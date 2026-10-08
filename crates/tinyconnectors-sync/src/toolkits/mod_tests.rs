//! Tests for the shipped toolkit providers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::default_registry;
use crate::Result;
use crate::provider::{ActionRunner, ProviderContext};
use crate::scope::{ToolScope, classify_unknown, find_curated, toolkit_from_slug};

#[derive(Debug)]
struct FixedActions {
    payload: serde_json::Value,
    last_action: Mutex<Option<String>>,
    last_arguments: Mutex<Option<serde_json::Value>>,
}

#[async_trait]
impl ActionRunner for FixedActions {
    async fn run(
        &self,
        action: &str,
        arguments: serde_json::Value,
        _: &str,
    ) -> Result<serde_json::Value> {
        *self.last_action.lock().unwrap() = Some(action.to_string());
        *self.last_arguments.lock().unwrap() = Some(arguments);
        Ok(self.payload.clone())
    }
}

fn context(toolkit: &str, payload: serde_json::Value) -> (Arc<FixedActions>, ProviderContext) {
    let actions = Arc::new(FixedActions {
        payload,
        last_action: Mutex::new(None),
        last_arguments: Mutex::new(None),
    });
    let context = ProviderContext {
        toolkit: toolkit.to_string(),
        connection_id: "conn_1".into(),
        actions: actions.clone(),
    };
    (actions, context)
}

#[test]
fn ships_every_toolkit_that_has_a_curated_catalog() {
    let registry = default_registry();
    for toolkit in ["gmail", "github", "notion", "linear", "clickup", "slack"] {
        assert!(registry.get(toolkit).is_some(), "{toolkit} must be shipped");
    }
    assert_eq!(registry.agent_ready_toolkits().len(), registry.len());
}

#[test]
fn every_provider_reports_a_slug_matching_its_registry_key() {
    // The registry keys on the slug; a mismatch means the provider is simply
    // never found, with no error anywhere.
    for provider in default_registry().all() {
        let slug = provider.toolkit_slug();
        assert_eq!(slug, slug.trim().to_ascii_lowercase(), "{slug}");
        assert_ne!(slug.len(), 0);
    }
}

#[test]
fn every_provider_describes_itself() {
    // The description is rendered in the capability matrix, so a blank one
    // shows the user an empty row.
    for provider in default_registry().all() {
        let description = provider.description();
        assert!(
            !description.trim().is_empty(),
            "{}",
            provider.toolkit_slug()
        );
        assert!(description.ends_with('.'), "{}", provider.toolkit_slug());
    }
}

#[test]
fn every_curated_action_belongs_to_its_own_toolkit() {
    // A stray slug in the wrong catalog is invisible until an agent calls it
    // against an account that cannot run it.
    for provider in default_registry().all() {
        let toolkit = provider.toolkit_slug();
        for tool in provider.curated_tools().unwrap_or_default() {
            assert_eq!(
                toolkit_from_slug(tool.slug).as_deref(),
                Some(toolkit),
                "{} is in the {toolkit} catalog",
                tool.slug
            );
        }
    }
}

#[test]
fn no_catalog_lists_the_same_action_twice() {
    for provider in default_registry().all() {
        let catalog = provider.curated_tools().unwrap_or_default();
        let mut slugs: Vec<_> = catalog.iter().map(|tool| tool.slug).collect();
        slugs.sort_unstable();
        let mut deduplicated = slugs.clone();
        deduplicated.dedup();
        assert_eq!(slugs, deduplicated, "{}", provider.toolkit_slug());
    }
}

#[test]
fn no_curated_action_is_scoped_less_invasively_than_its_verb() {
    // Curation may be stricter than the heuristic — a judgement call about a
    // particular action — but never looser. A `DELETE` action tagged `read`
    // would be offered to a user who allowed only reads.
    for provider in default_registry().all() {
        for tool in provider.curated_tools().unwrap_or_default() {
            let heuristic = classify_unknown(tool.slug);
            assert!(
                tool.scope >= heuristic,
                "{} is curated as {:?} but its verb reads as {:?}",
                tool.slug,
                tool.scope,
                heuristic
            );
        }
    }
}

#[test]
fn every_catalog_offers_something_to_read() {
    // A catalog of writes alone cannot support a sync or answer a question.
    for provider in default_registry().all() {
        let catalog = provider.curated_tools().unwrap_or_default();
        assert!(
            catalog.iter().any(|tool| tool.scope == ToolScope::Read),
            "{} offers no read action",
            provider.toolkit_slug()
        );
    }
}

#[tokio::test]
async fn gmail_reads_its_identity_from_the_profile_action() {
    let (actions, context) = context(
        "gmail",
        json!({ "emailAddress": "user@example.com", "messagesTotal": 42 }),
    );
    let profile = default_registry()
        .get("gmail")
        .unwrap()
        .fetch_user_profile(&context)
        .await
        .unwrap();

    assert_eq!(
        actions.last_action.lock().unwrap().as_deref(),
        Some("GMAIL_GET_PROFILE")
    );
    assert_eq!(profile.toolkit, "gmail");
    assert_eq!(profile.email.as_deref(), Some("user@example.com"));
    assert_eq!(profile.connection_id.as_deref(), Some("conn_1"));
    // The raw payload survives, so a caller wanting `messagesTotal` can have it
    // without this shape growing a field for every toolkit's extras.
    assert_eq!(profile.extras["messagesTotal"], 42);
}

#[tokio::test]
async fn github_prefers_the_display_name_and_keeps_the_login() {
    let (_actions, context) = context(
        "github",
        json!({
            "login": "octocat",
            "name": "The Octocat",
            "avatar_url": "https://example.com/a.png",
            "html_url": "https://github.com/octocat"
        }),
    );
    let profile = default_registry()
        .get("github")
        .unwrap()
        .fetch_user_profile(&context)
        .await
        .unwrap();

    assert_eq!(profile.username.as_deref(), Some("octocat"));
    assert_eq!(profile.display_name.as_deref(), Some("The Octocat"));
    assert_eq!(
        profile.profile_url.as_deref(),
        Some("https://github.com/octocat")
    );
}

#[tokio::test]
async fn notion_reads_a_nested_email() {
    let (_actions, context) = context(
        "notion",
        json!({ "name": "Ada", "person": { "email": "ada@example.com" } }),
    );
    let profile = default_registry()
        .get("notion")
        .unwrap()
        .fetch_user_profile(&context)
        .await
        .unwrap();

    assert_eq!(profile.display_name.as_deref(), Some("Ada"));
    assert_eq!(profile.email.as_deref(), Some("ada@example.com"));
}

#[tokio::test]
async fn a_provider_reporting_nothing_yields_an_empty_profile_not_an_error() {
    // A toolkit that answers its profile action with an unexpected shape is
    // still connected. Failing here would present it as broken.
    let (_actions, context) = context("linear", json!({}));
    let profile = default_registry()
        .get("linear")
        .unwrap()
        .fetch_user_profile(&context)
        .await
        .unwrap();

    assert_eq!(profile.toolkit, "linear");
    assert!(profile.email.is_none());
    assert!(profile.display_name.is_none());
}

#[test]
fn a_known_action_resolves_through_its_catalog() {
    let gmail = default_registry().get("gmail").unwrap();
    let catalog = gmail.curated_tools().unwrap();
    assert_eq!(
        find_curated(catalog, "GMAIL_FETCH_EMAILS").unwrap().scope,
        ToolScope::Read
    );
    assert_eq!(
        find_curated(catalog, "GMAIL_SEND_EMAIL").unwrap().scope,
        ToolScope::Write
    );
}

#[test]
fn every_toolkit_reports_a_complete_capability_row() {
    for row in default_registry().capabilities().capabilities {
        assert!(row.curated_tools, "{}", row.toolkit);
        assert!(row.tool_execution, "{}", row.toolkit);
        assert!(row.user_profile, "{}", row.toolkit);
    }
}

#[tokio::test]
async fn clickup_reads_its_username_and_avatar() {
    let (_actions, context) = context(
        "clickup",
        json!({
            "username": "ada",
            "email": "ada@example.com",
            "profilePicture": "https://example.com/a.png"
        }),
    );
    let profile = default_registry()
        .get("clickup")
        .unwrap()
        .fetch_user_profile(&context)
        .await
        .unwrap();

    assert_eq!(profile.username.as_deref(), Some("ada"));
    assert_eq!(profile.email.as_deref(), Some("ada@example.com"));
    assert_eq!(
        profile.avatar_url.as_deref(),
        Some("https://example.com/a.png")
    );
}

#[tokio::test]
async fn linear_reads_its_display_name() {
    let (_actions, context) = context("linear", json!({ "name": "Ada", "email": "a@b.com" }));
    let profile = default_registry()
        .get("linear")
        .unwrap()
        .fetch_user_profile(&context)
        .await
        .unwrap();
    assert_eq!(profile.display_name.as_deref(), Some("Ada"));
}

#[tokio::test]
async fn slack_reads_its_workspace_as_the_identity() {
    let (actions, context) = context(
        "slack",
        json!({ "team": { "name": "Acme", "domain": "acme", "icon": { "image_132": "https://example.com/i.png" } } }),
    );
    let profile = default_registry()
        .get("slack")
        .unwrap()
        .fetch_user_profile(&context)
        .await
        .unwrap();
    assert_eq!(
        actions.last_action.lock().unwrap().as_deref(),
        Some("SLACK_FETCH_TEAM_INFO")
    );
    assert_eq!(profile.toolkit, "slack");
    assert_eq!(profile.display_name.as_deref(), Some("Acme"));
    assert_eq!(profile.username.as_deref(), Some("acme"));
    assert_eq!(
        profile.avatar_url.as_deref(),
        Some("https://example.com/i.png")
    );
}
