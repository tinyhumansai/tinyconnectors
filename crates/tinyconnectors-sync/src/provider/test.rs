//! Unit tests for the provider abstraction and registry.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tinyconnectors_bus::ConnectorRecordBatch;

use super::{
    ActionRunner, ConnectorProvider, ProviderContext, ProviderRegistry, ProviderUserProfile,
    SyncLimits, SyncReason,
};
use crate::scope::{CuratedTool, ToolScope};
use crate::state::{SyncState, SyncStateStore};
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
        self.calls.lock().unwrap().push((
            action.to_string(),
            arguments,
            connection_id.to_string(),
        ));
        Ok(self.reply.lock().unwrap().clone())
    }
}

#[derive(Debug, Default)]
struct NullStore;

#[async_trait]
impl SyncStateStore for NullStore {
    async fn get(&self, _: &str, _: &str) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }
    async fn set(&self, _: &str, _: &str, _: &serde_json::Value) -> Result<()> {
        Ok(())
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
    can_sync: bool,
}

#[async_trait]
impl ConnectorProvider for TestProvider {
    fn toolkit_slug(&self) -> &'static str {
        self.slug
    }
    fn description(&self) -> &'static str {
        "a provider for tests"
    }
    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        self.curated
    }
    fn can_sync(&self) -> bool {
        self.can_sync
    }
    async fn fetch_user_profile(
        &self,
        context: &ProviderContext,
    ) -> Result<ProviderUserProfile> {
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
        can_sync: true,
    })
}

fn context(actions: Arc<FakeActions>) -> ProviderContext {
    ProviderContext {
        toolkit: "gmail".into(),
        connection_id: "conn_1".into(),
        source_id: "gmail:primary".into(),
        limits: SyncLimits::default(),
        actions,
        state: Arc::new(NullStore),
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
            can_sync: false,
        }));

    assert_eq!(registry.len(), 1);
    assert!(!registry.get("gmail").unwrap().can_sync());
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
            can_sync: true,
        }))
        .with(Arc::new(TestProvider {
            slug: "slack",
            // An empty catalog is not a catalog: the agent has nothing to call.
            curated: Some(&[]),
            can_sync: true,
        }));

    assert_eq!(registry.agent_ready_toolkits(), vec!["gmail".to_string()]);
}

// ── context ─────────────────────────────────────────────────────────

#[tokio::test]
async fn running_an_action_targets_this_run_s_connection() {
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
    assert_eq!(connection, "conn_1", "a provider cannot address another account");
}

#[test]
fn the_context_debug_output_hides_the_seams() {
    // Both are host implementations whose own `Debug` could print anything —
    // the action runner wraps a client holding a credential.
    let context = context(Arc::new(FakeActions::default()));
    let rendered = format!("{context:?}");
    assert!(rendered.contains("gmail"));
    assert!(!rendered.contains("FakeActions"), "{rendered}");
    assert!(!rendered.contains("NullStore"), "{rendered}");
}

#[test]
fn the_default_limits_make_a_first_sync_finish() {
    // A first sync of a years-old mailbox is otherwise unbounded: it costs
    // money per request and buries what the user wanted in a backfill.
    let limits = SyncLimits::default();
    assert!(limits.max_items > 0 && limits.max_items <= 1000);
    assert!(limits.depth_days.is_some());
}

// ── defaults ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_provider_that_cannot_sync_produces_an_empty_completed_batch() {
    let provider = TestProvider {
        slug: "slack",
        curated: None,
        can_sync: false,
    };
    let batch: ConnectorRecordBatch = provider
        .fetch_records(&context(Arc::new(FakeActions::default())))
        .await
        .unwrap();

    assert!(batch.records.is_empty());
    assert!(batch.complete, "an empty run must not look like more to come");
    assert_eq!(batch.toolkit, "gmail");
    assert_eq!(batch.connection_id.as_deref(), Some("conn_1"));
}

#[test]
fn every_sync_reason_has_a_stable_wire_name() {
    for (reason, name) in [
        (SyncReason::InitialConnect, "initial_connect"),
        (SyncReason::Scheduled, "scheduled"),
        (SyncReason::Manual, "manual"),
        (SyncReason::Trigger, "trigger"),
    ] {
        assert_eq!(reason.as_str(), name);
        assert_eq!(serde_json::to_value(reason).unwrap(), json!(name));
    }
}

#[tokio::test]
async fn state_flows_through_the_context() {
    let context = context(Arc::new(FakeActions::default()));
    let state = SyncState::load(context.state.as_ref(), "gmail", "conn_1")
        .await
        .unwrap();
    assert_eq!(state.toolkit, "gmail");
    assert!(state.cursor.is_none());
}

#[test]
fn an_action_failure_names_the_action() {
    let error = Error::Action {
        action: "GMAIL_FETCH_EMAILS".into(),
        message: "insufficient scope".into(),
    };
    assert!(error.to_string().contains("GMAIL_FETCH_EMAILS"));
}
