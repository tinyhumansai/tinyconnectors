//! Unit tests for the provider abstraction and registry.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::{ActionRunner, ConnectorProvider, ProviderContext, ProviderRegistry, ProviderUserProfile};
use crate::scope::{CuratedTool, ToolScope};
use crate::{Error, Result};

// ── doubles ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct FakeActions {
    reply: Mutex<serde_json::Value>,
    calls: Mutex<Vec<(String, serde_json::Value, String)>>,
}

#[async_trait]
impl ActionRunner for FakeActions {
    async fn run(
        &self,
        action: &str,
        arguments: serde_json::Value,
        connection_id: &str,
    ) -> Result<serde_json::Value> {
        self.calls
            .lock()
            .unwrap()
            .push((action.to_string(), arguments, connection_id.to_string()));
        Ok(self.reply.lock().unwrap().clone())
    }
}

const CURATED: &[CuratedTool] = &[CuratedTool {
    slug: "GMAIL_FETCH_EMAILS",
    scope: ToolScope::Read,
}];

#[derive(Debug)]
struct TestProvider {
    slug: &'static str,
    curated: Option<&'static [CuratedTool]>,
    description: &'static str,
}

#[async_trait]
impl ConnectorProvider for TestProvider {
    fn toolkit_slug(&self) -> &'static str {
        self.slug
    }
    fn description(&self) -> &'static str {
        self.description
    }
    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        self.curated
    }
    async fn fetch_user_profile(&self, context: &ProviderContext) -> Result<ProviderUserProfile> {
        let raw = context.run("TEST_GET_PROFILE", json!({})).await?;
        Ok(ProviderUserProfile {
            toolkit: context.toolkit.clone(),
            connection_id: Some(context.connection_id.clone()),
            email: raw
                .get("email")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            ..ProviderUserProfile::default()
        })
    }
}

fn provider(slug: &'static str) -> Arc<dyn ConnectorProvider> {
    Arc::new(TestProvider {
        slug,
        curated: Some(CURATED),
        description: "a provider for tests",
    })
}

fn context(actions: Arc<FakeActions>) -> ProviderContext {
    ProviderContext {
        toolkit: "gmail".into(),
        connection_id: "conn_1".into(),
        actions,
    }
}

// ── registry ────────────────────────────────────────────────────────

#[test]
fn an_empty_registry_finds_nothing() {
    let registry = ProviderRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(registry.get("gmail").is_none());
}

#[test]
fn finds_a_provider_by_its_slug() {
    let registry = ProviderRegistry::new().with(provider("gmail"));
    assert_eq!(registry.get("gmail").unwrap().toolkit_slug(), "gmail");
}

#[test]
fn normalizes_the_slug_before_looking_up() {
    // The toolkit reaches here from a config file, a UI field, and a backend
    // envelope; only one of those three is reliably normalized.
    let registry = ProviderRegistry::new().with(provider("gmail"));
    for spelling in ["GMAIL", " Gmail ", "gmail"] {
        assert!(registry.get(spelling).is_some(), "{spelling}");
    }
}

#[test]
fn registering_the_same_toolkit_replaces_rather_than_duplicates() {
    // A duplicate registration is far more often a deliberate override than a
    // mistake, and refusing it would make a host remove the built-in first.
    let registry = ProviderRegistry::new()
        .with(provider("gmail"))
        .with(Arc::new(TestProvider {
            slug: "gmail",
            curated: None,
            description: "an override",
        }));

    assert_eq!(registry.len(), 1);
    assert_eq!(registry.get("gmail").unwrap().description(), "an override");
}

#[test]
fn lists_providers_in_a_stable_order() {
    // The capability matrix is rendered from this. A set that reshuffles makes
    // a UI list jump between runs for no reason.
    let registry = ProviderRegistry::new()
        .with(provider("slack"))
        .with(provider("gmail"))
        .with(provider("notion"));

    let slugs: Vec<_> = registry
        .all()
        .iter()
        .map(|provider| provider.toolkit_slug())
        .collect();
    assert_eq!(slugs, ["gmail", "notion", "slack"]);
}

#[test]
fn reports_only_toolkits_with_a_curated_catalog_as_agent_ready() {
    let registry = ProviderRegistry::new()
        .with(provider("gmail"))
        .with(Arc::new(TestProvider {
            slug: "notion",
            curated: None,
            description: "a provider for tests",
        }))
        .with(Arc::new(TestProvider {
            slug: "slack",
            // An empty catalog is not a catalog: the agent has nothing to call.
            curated: Some(&[]),
            description: "a provider for tests",
        }));

    assert_eq!(registry.agent_ready_toolkits(), vec!["gmail".to_string()]);
}

// ── context ─────────────────────────────────────────────────────────

#[tokio::test]
async fn running_an_action_targets_this_contexts_connection() {
    let actions = Arc::new(FakeActions {
        reply: Mutex::new(json!({ "email": "user@example.com" })),
        ..FakeActions::default()
    });
    let context = context(actions.clone());

    let profile = provider("gmail")
        .fetch_user_profile(&context)
        .await
        .unwrap();
    assert_eq!(profile.email.as_deref(), Some("user@example.com"));

    let (action, _, connection) = actions.calls.lock().unwrap()[0].clone();
    assert_eq!(action, "TEST_GET_PROFILE");
    assert_eq!(
        connection, "conn_1",
        "a provider cannot address another account"
    );
}

#[test]
fn the_context_debug_output_hides_the_action_runner() {
    // The action runner is a host implementation whose own `Debug` could print
    // anything — it wraps a client holding a credential.
    let context = context(Arc::new(FakeActions::default()));
    let rendered = format!("{context:?}");
    assert!(rendered.contains("gmail"));
    assert!(!rendered.contains("FakeActions"), "{rendered}");
}

#[test]
fn the_capability_matrix_describes_the_build_not_the_user() {
    let registry = ProviderRegistry::new()
        .with(provider("gmail"))
        .with(Arc::new(TestProvider {
            slug: "slack",
            curated: None,
            description: "an override",
        }));

    let rows = registry.capabilities().capabilities;
    assert_eq!(rows.len(), 2);

    let gmail = &rows[0];
    assert_eq!(gmail.toolkit, "gmail");
    assert!(gmail.curated_tools);
    assert_eq!(gmail.curated_tool_count, 1);

    // An uncurated toolkit is connectable and usable, but the agent is offered
    // no hand-picked catalog for it.
    let slack = &rows[1];
    assert!(!slack.curated_tools);
    assert!(slack.tool_execution, "it can still be acted through");
}

#[test]
fn an_action_failure_names_the_action() {
    let error = Error::Action {
        action: "GMAIL_FETCH_EMAILS".into(),
        message: "insufficient scope".into(),
    };
    assert!(error.to_string().contains("GMAIL_FETCH_EMAILS"));
}
