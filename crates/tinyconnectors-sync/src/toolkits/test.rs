//! Tests for the shipped toolkit providers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::default_registry;
use crate::Result;
use crate::provider::{ActionRunner, ProviderContext, SyncLimits};
use crate::scope::{ToolScope, classify_unknown, find_curated, toolkit_from_slug};
use crate::state::SyncStateStore;

#[derive(Debug)]
struct FixedActions {
    payload: serde_json::Value,
    last_action: Mutex<Option<String>>,
}

#[async_trait]
impl ActionRunner for FixedActions {
    async fn run(&self, action: &str, _: serde_json::Value, _: &str) -> Result<serde_json::Value> {
        *self.last_action.lock().unwrap() = Some(action.to_string());
        Ok(self.payload.clone())
    }
}

#[derive(Debug)]
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

fn context(toolkit: &str, payload: serde_json::Value) -> (Arc<FixedActions>, ProviderContext) {
    let actions = Arc::new(FixedActions {
        payload,
        last_action: Mutex::new(None),
    });
    let context = ProviderContext {
        toolkit: toolkit.to_string(),
        connection_id: "conn_1".into(),
        source_id: format!("{toolkit}:conn_1"),
        limits: SyncLimits::default(),
        actions: actions.clone(),
        state: Arc::new(NullStore),
    };
    (actions, context)
}

#[test]
fn ships_every_toolkit_that_has_a_curated_catalog() {
    let registry = default_registry();
    for toolkit in ["gmail", "github", "notion", "linear", "clickup"] {
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
        assert!(!slug.is_empty());
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
        assert!(row.initial_sync, "{}", row.toolkit);
        assert!(row.periodic_sync, "{}", row.toolkit);
        assert!(row.memory_ingest, "{}", row.toolkit);
    }
}

#[tokio::test]
async fn every_toolkit_reads_a_page_into_records() {
    // The payload shapes differ per toolkit, so each is given the envelope its
    // own spec names. What is checked is that the spec and the reader agree.
    let payloads = [
        (
            "gmail",
            json!({ "data": { "messages": [{ "id": "m1", "subject": "Hi", "snippet": "there" }] } }),
        ),
        (
            "github",
            json!({ "data": { "items": [{ "id": 7, "title": "A bug", "body": "steps" }] } }),
        ),
        (
            "notion",
            json!({ "data": { "results": [{ "id": "p1", "title": "Notes", "content": "text" }] } }),
        ),
        (
            "linear",
            json!({ "data": { "issues": [{ "id": "i1", "title": "Task", "description": "do it" }] } }),
        ),
        (
            "clickup",
            json!({ "data": { "tasks": [{ "id": "t1", "name": "Chore", "description": "soon" }] } }),
        ),
    ];

    for (toolkit, payload) in payloads {
        let (_actions, context) = context(toolkit, payload);
        let page = default_registry()
            .get(toolkit)
            .unwrap()
            .fetch_page(&context, None)
            .await
            .unwrap();

        assert_eq!(page.records.len(), 1, "{toolkit} read no record");
        assert!(!page.records[0].item_id.is_empty(), "{toolkit} has no id");
        assert!(!page.records[0].title.is_empty(), "{toolkit} has no title");
        assert!(!page.records[0].content.is_empty(), "{toolkit} has no body");
    }
}

#[tokio::test]
async fn every_toolkit_asks_its_own_fetch_action() {
    for toolkit in ["gmail", "github", "notion", "linear", "clickup"] {
        let (actions, context) = context(toolkit, json!({}));
        default_registry()
            .get(toolkit)
            .unwrap()
            .fetch_page(&context, None)
            .await
            .unwrap();

        let action = actions.last_action.lock().unwrap().clone().unwrap();
        assert_eq!(
            crate::scope::toolkit_from_slug(&action).as_deref(),
            Some(toolkit),
            "{toolkit} fetches with {action}, which belongs to another toolkit"
        );
    }
}

#[tokio::test]
async fn a_github_numeric_id_becomes_a_record_id() {
    // GitHub reports ids as numbers; a dedupe key has to be a string, and
    // dropping the record for its type would lose every issue.
    let (_actions, context) = context("github", json!({ "data": { "items": [{ "id": 42 }] } }));
    let page = default_registry()
        .get("github")
        .unwrap()
        .fetch_page(&context, None)
        .await
        .unwrap();
    assert_eq!(page.records[0].item_id, "42");
}
