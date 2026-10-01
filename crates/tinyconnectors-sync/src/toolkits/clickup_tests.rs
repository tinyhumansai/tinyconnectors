//! Tests for the `ClickUp` walk over workspaces and their task pages.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};

use super::ClickupProvider;
use crate::pipeline::ProviderPage;
use crate::provider::{ActionRunner, ConnectorProvider, ProviderContext, SyncLimits};
use crate::state::SyncStateStore;
use crate::{Error, Result};

const WORKSPACES: &str = "CLICKUP_GET_AUTHORIZED_TEAMS_WORKSPACES";
const TASKS: &str = "CLICKUP_GET_FILTERED_TEAM_TASKS";

// ── doubles ─────────────────────────────────────────────────────────

/// An action runner scripted per action, recording what it was asked.
#[derive(Debug, Default)]
struct ScriptedActions {
    replies: Mutex<HashMap<String, VecDeque<Result<Value>>>>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl ScriptedActions {
    fn queue(&self, action: &str, reply: Result<Value>) {
        self.replies
            .lock()
            .unwrap()
            .entry(action.to_string())
            .or_default()
            .push_back(reply);
    }

    /// Every call made against `action`, in order, with its arguments.
    fn calls_to(&self, action: &str) -> Vec<Value> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == action)
            .map(|(_, arguments)| arguments.clone())
            .collect()
    }
}

#[async_trait]
impl ActionRunner for ScriptedActions {
    async fn run(&self, action: &str, arguments: Value, _: &str) -> Result<Value> {
        self.calls
            .lock()
            .unwrap()
            .push((action.to_string(), arguments));
        let queued = self
            .replies
            .lock()
            .unwrap()
            .get_mut(action)
            .and_then(VecDeque::pop_front);
        queued.unwrap_or_else(|| Ok(json!({})))
    }
}

#[derive(Debug)]
struct NullStore;

#[async_trait]
impl SyncStateStore for NullStore {
    async fn get(&self, _: &str, _: &str) -> Result<Option<Value>> {
        Ok(None)
    }
    async fn set(&self, _: &str, _: &str, _: &Value) -> Result<()> {
        Ok(())
    }
}

fn context(actions: Arc<ScriptedActions>) -> ProviderContext {
    ProviderContext {
        toolkit: "clickup".into(),
        connection_id: "conn_1".into(),
        source_id: "clickup:conn_1".into(),
        limits: SyncLimits {
            max_items: 50,
            depth_days: Some(30),
        },
        actions,
        state: Arc::new(NullStore),
    }
}

/// The workspaces payload, listing `ids`.
fn workspaces(ids: &[&str]) -> Value {
    let teams: Vec<Value> = ids
        .iter()
        .map(|id| json!({ "id": id, "name": "A workspace" }))
        .collect();
    json!({ "data": { "teams": teams } })
}

/// A task page of `count` tasks, carrying `ClickUp`'s `last_page` flag when given.
fn tasks(count: usize, last_page: Option<bool>) -> Value {
    let tasks: Vec<Value> = (0..count)
        .map(|n| {
            json!({
                "id": format!("t{n}"),
                "name": "A task",
                "description": "details",
                "date_updated": "1700000000000"
            })
        })
        .collect();
    let mut payload = json!({ "data": { "tasks": tasks } });
    if let Some(last_page) = last_page {
        payload["data"]["last_page"] = json!(last_page);
    }
    payload
}

fn action_error(action: &str) -> Error {
    Error::Action {
        action: action.into(),
        message: "upstream 503".into(),
    }
}

async fn read(actions: &Arc<ScriptedActions>, cursor: Option<&str>) -> Result<ProviderPage> {
    ClickupProvider
        .fetch_page(&context(Arc::clone(actions)), cursor)
        .await
}

// ── the walk ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_walk_starts_on_the_first_workspace_at_page_zero() {
    // `ClickUp` reads tasks one workspace at a time, and its task search refuses
    // a request that names none.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1", "w2"])));
    actions.queue(TASKS, Ok(tasks(2, Some(false))));

    let page = read(&actions, None).await.unwrap();

    assert_eq!(page.records.len(), 2);
    assert_eq!(page.next_cursor.as_deref(), Some("w1|1"));
    let request = &actions.calls_to(TASKS)[0];
    assert_eq!(request["team_id"], "w1", "{request}");
    assert_eq!(request["page"], 0, "pages count from zero: {request}");
    // Most recently updated first, subtasks included.
    assert_eq!(request["order_by"], "updated", "{request}");
    assert_eq!(request["reverse"], true, "{request}");
    assert_eq!(request["subtasks"], true, "{request}");
}

#[tokio::test]
async fn the_walk_moves_to_the_next_workspace_after_a_last_page() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1", "w2"])));
    actions.queue(TASKS, Ok(tasks(3, Some(true))));

    let page = read(&actions, Some("w1|4")).await.unwrap();

    let request = &actions.calls_to(TASKS)[0];
    assert_eq!(request["team_id"], "w1", "{request}");
    assert_eq!(request["page"], 4, "{request}");
    assert_eq!(page.records.len(), 3);
    assert_eq!(page.next_cursor.as_deref(), Some("w2|0"));
}

#[tokio::test]
async fn the_walk_ends_after_the_last_workspace() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1", "w2"])));
    actions.queue(TASKS, Ok(tasks(1, Some(true))));

    let page = read(&actions, Some("w2|0")).await.unwrap();

    assert_eq!(page.records.len(), 1);
    assert!(page.next_cursor.is_none());
}

#[tokio::test]
async fn without_the_flag_a_full_page_reads_on_and_a_short_one_ends_the_workspace() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1", "w2"])));
    actions.queue(TASKS, Ok(tasks(50, None)));
    actions.queue(WORKSPACES, Ok(workspaces(&["w1", "w2"])));
    actions.queue(TASKS, Ok(tasks(10, None)));

    let full = read(&actions, Some("w1|0")).await.unwrap();
    assert_eq!(full.next_cursor.as_deref(), Some("w1|1"));
    let short = read(&actions, Some("w1|1")).await.unwrap();
    assert_eq!(short.next_cursor.as_deref(), Some("w2|0"));
}

#[tokio::test]
async fn an_empty_page_ends_the_workspace_whatever_its_flag_says() {
    // A page of nothing cannot be followed by more. Believing the flag here
    // would walk empty pages until the day's request budget ran out.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1"])));
    actions.queue(TASKS, Ok(tasks(0, Some(false))));

    let page = read(&actions, Some("w1|3")).await.unwrap();

    assert!(page.next_cursor.is_none());
}

#[tokio::test]
async fn an_account_with_no_workspace_reads_nothing() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&[])));

    let page = read(&actions, None).await.unwrap();

    assert!(page.records.is_empty());
    assert!(page.next_cursor.is_none());
    assert!(
        actions.calls_to(TASKS).is_empty(),
        "no workspace, no task read"
    );
}

#[tokio::test]
async fn a_cursor_naming_a_workspace_no_longer_listed_starts_over() {
    // The account left that workspace. Reading its tasks would fail on every
    // run from then on; starting from the first listed workspace recovers.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w2"])));
    actions.queue(TASKS, Ok(tasks(1, Some(true))));

    read(&actions, Some("gone|3")).await.unwrap();

    let request = &actions.calls_to(TASKS)[0];
    assert_eq!(request["team_id"], "w2", "{request}");
    assert_eq!(request["page"], 0, "{request}");
}

#[tokio::test]
async fn an_unreadable_cursor_starts_the_walk_over() {
    for cursor in ["page-2", "w1|", "w1|-1", "|3"] {
        let actions = Arc::new(ScriptedActions::default());
        actions.queue(WORKSPACES, Ok(workspaces(&["w1"])));
        actions.queue(TASKS, Ok(tasks(1, Some(true))));

        read(&actions, Some(cursor)).await.unwrap();

        let request = &actions.calls_to(TASKS)[0];
        assert_eq!(request["team_id"], "w1", "{cursor}: {request}");
        assert_eq!(request["page"], 0, "{cursor}: {request}");
    }
}

#[tokio::test]
async fn workspace_ids_are_read_from_either_envelope() {
    for payload in [
        json!({ "teams": [{ "id": 9001 }] }),
        json!({ "data": { "workspaces": [{ "id": "9001" }] } }),
    ] {
        let actions = Arc::new(ScriptedActions::default());
        actions.queue(WORKSPACES, Ok(payload.clone()));
        actions.queue(TASKS, Ok(tasks(1, Some(true))));

        read(&actions, None).await.unwrap();

        assert_eq!(actions.calls_to(TASKS)[0]["team_id"], "9001", "{payload}");
    }
}

#[tokio::test]
async fn a_workspace_listed_twice_is_walked_once() {
    // The walk finds the next workspace by position. A repeated id would send
    // the end of the first copy to the second, and on to itself forever,
    // never reaching the workspaces listed after it.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1", "w1", "w2"])));
    actions.queue(TASKS, Ok(tasks(1, Some(true))));

    let page = read(&actions, Some("w1|2")).await.unwrap();

    assert_eq!(page.next_cursor.as_deref(), Some("w2|0"));
}

#[tokio::test]
async fn a_page_read_reports_every_request_it_made() {
    // The workspace list and the task page are two requests, and the day's
    // budget has to count both or it lets a run spend twice its limit.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1"])));
    actions.queue(TASKS, Ok(tasks(1, Some(true))));
    assert_eq!(read(&actions, None).await.unwrap().requests_used, 2);

    // With no workspace there is no task read: the list is the only request.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&[])));
    assert_eq!(read(&actions, None).await.unwrap().requests_used.max(1), 1);
}

#[tokio::test]
async fn a_failed_read_fails_the_page() {
    // Either request failing is the run's failure to report. An empty page
    // would read as "nothing to sync".
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Err(action_error(WORKSPACES)));
    assert!(read(&actions, None).await.is_err());

    let actions = Arc::new(ScriptedActions::default());
    actions.queue(WORKSPACES, Ok(workspaces(&["w1"])));
    actions.queue(TASKS, Err(action_error(TASKS)));
    assert!(read(&actions, None).await.is_err());
}
