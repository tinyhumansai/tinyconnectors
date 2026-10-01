//! Tests for the Slack provider's channel walk.
//!
//! The shared toolkit tests cannot drive this one: their action double answers
//! every action with the same canned payload, and a Slack page read needs the
//! conversation list, the user directory and the history to answer differently.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{Cursor, SlackProvider};
use crate::pipeline::run_sync;
use crate::provider::{ActionRunner, ConnectorProvider, ProviderContext, SyncLimits, SyncReason};
use crate::state::SyncStateStore;
use crate::{Error, Result};

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

/// A state store that actually remembers, so a walk can resume.
#[derive(Debug, Default)]
struct MemoryStore {
    rows: Mutex<HashMap<(String, String), Value>>,
}

#[async_trait]
impl SyncStateStore for MemoryStore {
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<Value>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(&(namespace.to_string(), key.to_string()))
            .cloned())
    }

    async fn set(&self, namespace: &str, key: &str, value: &Value) -> Result<()> {
        self.rows
            .lock()
            .unwrap()
            .insert((namespace.to_string(), key.to_string()), value.clone());
        Ok(())
    }
}

fn context(actions: Arc<ScriptedActions>, store: Arc<MemoryStore>) -> ProviderContext {
    ProviderContext {
        toolkit: "slack".into(),
        connection_id: "conn_1".into(),
        source_id: "slack:conn_1".into(),
        limits: SyncLimits::default(),
        actions,
        state: store,
    }
}

fn action_error(message: &str) -> Error {
    Error::Action {
        action: "SLACK_FETCH_CONVERSATION_HISTORY".into(),
        message: message.into(),
    }
}

fn channels(entries: &[(&str, &str)], next: &str) -> Value {
    let list: Vec<Value> = entries
        .iter()
        .map(|(id, name)| json!({ "id": id, "name": name }))
        .collect();
    json!({ "data": { "channels": list, "response_metadata": { "next_cursor": next } } })
}

fn history(messages: &[Value], next: &str) -> Value {
    json!({ "data": { "messages": messages, "response_metadata": { "next_cursor": next } } })
}

fn message(ts: &str, text: &str) -> Value {
    json!({ "ts": ts, "user": "U1", "text": text })
}

/// Walk to the end, collecting every record id in the order it arrived.
async fn drain(context: &ProviderContext) -> Vec<String> {
    let provider = SlackProvider;
    let mut cursor = None;
    let mut seen = Vec::new();
    for _ in 0..12 {
        let page = provider
            .fetch_page(context, cursor.as_deref())
            .await
            .unwrap();
        seen.extend(page.records.iter().map(|record| record.item_id.clone()));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return seen,
        }
    }
    panic!("the walk did not terminate");
}

// ── the cursor ──────────────────────────────────────────────────────

#[test]
fn a_cursor_survives_a_round_trip() {
    let cursor = Cursor::new(3, Some("dXNlcjpVMDYx".into()));
    assert_eq!(Cursor::decode(Some(&cursor.encode())), cursor);

    let start = Cursor::new(0, None);
    assert_eq!(Cursor::decode(Some(&start.encode())), start);
}

#[test]
fn an_unreadable_cursor_restarts_the_walk_rather_than_failing_it() {
    // The seen-set turns a re-read into skips, so restarting costs requests.
    // Erroring would cost the connection every future sync.
    for raw in ["", "|", "garbage"] {
        assert_eq!(Cursor::decode(Some(raw)).index, 0, "{raw}");
    }
    assert_eq!(Cursor::decode(None), Cursor::new(0, None));
}

#[test]
fn an_index_that_does_not_parse_takes_the_rest_of_the_cursor_with_it() {
    // Keeping the history cursor beside an unreadable index would ask Slack to
    // resume channel zero from a page belonging to some other channel. It
    // refuses, the page reads as a channel failure, and that channel loses the
    // walk. Nothing beside a position nobody can identify means anything.
    let ruined = Cursor::decode(Some("not-a-number|hist|1700000000.000100|replies"));
    assert_eq!(ruined, Cursor::default());
    assert!(ruined.history.is_none());
    assert!(ruined.thread.is_none());
}

// ── the walk ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_walk_pages_each_channel_then_moves_to_the_next_and_ends_once() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng"), ("C2", "ops")], "")),
    );
    // C1 has two pages, C2 has one.
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000002.000100", "newest")], "page2")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000001.000100", "older")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000003.000100", "ops news")], "")),
    );

    let store = Arc::new(MemoryStore::default());
    let context = context(actions.clone(), store);
    let provider = SlackProvider;

    let mut cursor = None;
    let mut seen = Vec::new();
    let mut pages = 0;
    loop {
        let page = provider
            .fetch_page(&context, cursor.as_deref())
            .await
            .unwrap();
        seen.extend(page.records.iter().map(|record| record.item_id.clone()));
        pages += 1;
        assert!(pages < 10, "the walk did not terminate");
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert_eq!(
        seen,
        vec![
            "C1:1700000002.000100",
            "C1:1700000001.000100",
            "C2:1700000003.000100"
        ]
    );
    assert_eq!(
        actions.calls_to("SLACK_LIST_CONVERSATIONS").len(),
        1,
        "the roster is read once per walk, not once per channel"
    );
}

#[tokio::test]
async fn the_same_timestamp_in_two_channels_yields_two_records() {
    // A Slack `ts` is unique within a channel only, and the run loop dedupes
    // against one flat set for the whole connection.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng"), ("C2", "ops")], "")),
    );
    for _ in 0..2 {
        actions.queue(
            "SLACK_FETCH_CONVERSATION_HISTORY",
            Ok(history(&[message("1700000000.000100", "same")], "")),
        );
    }

    let context = context(actions, Arc::new(MemoryStore::default()));
    let provider = SlackProvider;

    let first = provider.fetch_page(&context, None).await.unwrap();
    let second = provider
        .fetch_page(&context, first.next_cursor.as_deref())
        .await
        .unwrap();

    assert_eq!(first.records[0].item_id, "C1:1700000000.000100");
    assert_eq!(second.records[0].item_id, "C2:1700000000.000100");
}

#[tokio::test]
async fn one_unreadable_channel_is_skipped_not_fatal() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "private"), ("C2", "ops")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Err(action_error("not_in_channel")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000000.000100", "readable")], "")),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let provider = SlackProvider;

    let skipped = provider.fetch_page(&context, None).await.unwrap();
    assert!(skipped.records.is_empty());
    let next = skipped.next_cursor.expect("the walk must move past it");

    let page = provider.fetch_page(&context, Some(&next)).await.unwrap();
    assert_eq!(page.records.len(), 1, "the other channel still syncs");
}

#[tokio::test]
async fn a_revoked_token_fails_the_run_instead_of_every_channel() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Err(action_error("token_revoked")),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    assert!(SlackProvider.fetch_page(&context, None).await.is_err());
}

#[tokio::test]
async fn a_finished_channel_is_read_from_its_newest_message_next_time() {
    let actions = Arc::new(ScriptedActions::default());
    for _ in 0..2 {
        actions.queue(
            "SLACK_LIST_CONVERSATIONS",
            Ok(channels(&[("C1", "eng")], "")),
        );
    }
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000009.000100", "newest")], "")),
    );
    actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));

    let store = Arc::new(MemoryStore::default());
    let context = context(actions.clone(), store);
    let provider = SlackProvider;

    // First walk: no mark yet, so the depth bound is the floor.
    provider.fetch_page(&context, None).await.unwrap();
    let first = &actions.calls_to("SLACK_FETCH_CONVERSATION_HISTORY")[0];
    assert_ne!(first["oldest"], json!("1700000009.000100"));

    // Second walk, after the channel was exhausted: start at the mark.
    provider.fetch_page(&context, None).await.unwrap();
    let second = &actions.calls_to("SLACK_FETCH_CONVERSATION_HISTORY")[1];
    assert_eq!(second["oldest"], json!("1700000009.000100"));
}

#[tokio::test]
async fn a_run_without_a_depth_bound_asks_for_everything() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));

    let mut context = context(actions.clone(), Arc::new(MemoryStore::default()));
    context.limits = SyncLimits {
        max_items: 50,
        depth_days: None,
    };

    SlackProvider.fetch_page(&context, None).await.unwrap();
    let arguments = &actions.calls_to("SLACK_FETCH_CONVERSATION_HISTORY")[0];
    assert!(
        arguments.get("oldest").is_none(),
        "an unbounded run must not invent a floor: {arguments}"
    );
}

#[tokio::test]
async fn a_join_notice_with_no_text_is_not_a_memory() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[
                json!({ "ts": "1700000000.000100", "subtype": "channel_join" }),
                message("1700000001.000100", "real content"),
            ],
            "",
        )),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let page = SlackProvider.fetch_page(&context, None).await.unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].content, "real content");
}

#[tokio::test]
async fn an_edited_message_reports_a_version_so_it_re_ingests() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[json!({
                "ts": "1700000000.000100",
                "user": "U1",
                "text": "fixed a typo",
                "edited": { "ts": "1700000050.000200" }
            })],
            "",
        )),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let page = SlackProvider.fetch_page(&context, None).await.unwrap();
    assert_eq!(
        page.versions,
        vec![(
            "C1:1700000000.000100".to_string(),
            "1700000050.000200".to_string()
        )]
    );
}

#[tokio::test]
async fn the_profile_names_the_workspace() {
    // A Slack connection is *of* a workspace; a UI labelling the account has
    // nothing better to show than its name.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_FETCH_TEAM_INFO",
        Ok(json!({
            "team": {
                "name": "Tiny Humans",
                "domain": "tinyhumans",
                "icon": { "image_132": "https://example.com/team.png" }
            }
        })),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let profile = SlackProvider.fetch_user_profile(&context).await.unwrap();

    assert_eq!(profile.toolkit, "slack");
    assert_eq!(profile.connection_id.as_deref(), Some("conn_1"));
    assert_eq!(profile.display_name.as_deref(), Some("Tiny Humans"));
    assert_eq!(profile.username.as_deref(), Some("tinyhumans"));
    assert_eq!(
        profile.avatar_url.as_deref(),
        Some("https://example.com/team.png")
    );
}

#[tokio::test]
async fn a_workspace_reporting_nothing_yields_an_empty_profile_not_an_error() {
    let context = context(
        Arc::new(ScriptedActions::default()),
        Arc::new(MemoryStore::default()),
    );
    let profile = SlackProvider.fetch_user_profile(&context).await.unwrap();
    assert_eq!(profile.toolkit, "slack");
    assert!(profile.display_name.is_none());
}

#[tokio::test]
async fn the_roster_is_walked_one_page_at_a_time() {
    // A workspace larger than one roster page costs one extra request per page,
    // not a loop inside a single page read.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "roster2")),
    );
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C2", "ops")], "")),
    );
    for ts in ["1700000001.000100", "1700000002.000100"] {
        actions.queue(
            "SLACK_FETCH_CONVERSATION_HISTORY",
            Ok(history(&[message(ts, "hello")], "")),
        );
    }

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));
    let provider = SlackProvider;

    let mut cursor = None;
    let mut seen = Vec::new();
    for _ in 0..6 {
        let page = provider
            .fetch_page(&context, cursor.as_deref())
            .await
            .unwrap();
        seen.extend(page.records.iter().map(|record| record.item_id.clone()));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    assert_eq!(seen, vec!["C1:1700000001.000100", "C2:1700000002.000100"]);
    assert_eq!(actions.calls_to("SLACK_LIST_CONVERSATIONS").len(), 2);
}

#[tokio::test]
async fn a_roster_cursor_that_does_not_advance_ends_the_walk() {
    // Slack repeating the cursor it was handed would otherwise walk the same
    // page until the daily budget ran out, ingesting nothing on every pass.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "stuck")),
    );
    actions.queue("SLACK_LIST_CONVERSATIONS", Ok(channels(&[], "stuck")));
    actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));
    let provider = SlackProvider;

    let mut cursor = None;
    let mut pages = 0;
    loop {
        let page = provider
            .fetch_page(&context, cursor.as_deref())
            .await
            .unwrap();
        pages += 1;
        assert!(pages < 8, "the walk did not terminate on a stuck cursor");
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(actions.calls_to("SLACK_LIST_CONVERSATIONS").len(), 2);
}

#[tokio::test]
async fn authors_and_mentions_read_as_names_and_the_directory_is_cached() {
    let actions = Arc::new(ScriptedActions::default());
    for _ in 0..2 {
        actions.queue(
            "SLACK_LIST_CONVERSATIONS",
            Ok(channels(&[("C1", "eng")], "")),
        );
    }
    actions.queue(
        "SLACK_LIST_ALL_USERS",
        Ok(json!({ "data": { "members": [
            { "id": "U1", "profile": { "display_name": "Ada" } },
            { "id": "U2", "real_name": "Grace" }
        ] } })),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[json!({ "ts": "1700000000.000100", "user": "U1", "text": "ping <@U2>" })],
            "",
        )),
    );
    actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));
    let provider = SlackProvider;

    let page = provider.fetch_page(&context, None).await.unwrap();
    assert_eq!(page.records[0].title, "#eng — Ada");
    assert_eq!(page.records[0].content, "ping @Grace");
    assert_eq!(
        page.records[0].url.as_deref(),
        Some("https://slack.com/archives/C1/p1700000000000100")
    );
    assert_eq!(page.records[0].updated_at_ms, Some(1_700_000_000_000));

    // A second walk reuses the directory rather than paying for it again.
    provider.fetch_page(&context, None).await.unwrap();
    assert_eq!(actions.calls_to("SLACK_LIST_ALL_USERS").len(), 1);
}

#[tokio::test]
async fn a_directory_that_will_not_load_leaves_ids_raw_rather_than_failing() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_LIST_ALL_USERS",
        Err(Error::Action {
            action: "SLACK_LIST_ALL_USERS".into(),
            message: "ratelimited".into(),
        }),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[json!({ "ts": "1700000000.000100", "user": "U9", "text": "hi <@U9>" })],
            "",
        )),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let page = SlackProvider.fetch_page(&context, None).await.unwrap();

    assert_eq!(page.records.len(), 1, "the walk still ingests");
    assert_eq!(page.records[0].content, "hi @U9");
    assert_eq!(page.records[0].title, "#eng — U9");
}

#[tokio::test]
async fn a_message_that_is_only_an_empty_reference_is_not_a_memory() {
    // Slack reports the odd message whose whole body resolves to nothing.
    // Ingesting it would fill a user's memory with blank rows.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000000.000100", "<>")], "")),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let page = SlackProvider.fetch_page(&context, None).await.unwrap();
    assert!(page.records.is_empty());
}

// ── through the real pipeline ───────────────────────────────────────

#[tokio::test]
async fn a_run_stops_at_the_item_limit_and_the_next_one_carries_on() {
    // The limit belongs to the run loop, not to this provider. Driving the
    // real `run_sync` is what proves the composite cursor is a cursor as far
    // as the pipeline is concerned.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng"), ("C2", "ops")], "")),
    );
    // C1 is read, truncates the run at one record, and is read again by the
    // next run — which skips what it already has and moves on to C2.
    for _ in 0..2 {
        actions.queue(
            "SLACK_FETCH_CONVERSATION_HISTORY",
            Ok(history(&[message("1700000001.000100", "first")], "")),
        );
    }
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng"), ("C2", "ops")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000002.000100", "second")], "")),
    );

    let store = Arc::new(MemoryStore::default());
    let mut context = context(actions, store);
    context.limits = SyncLimits {
        max_items: 1,
        depth_days: Some(30),
    };

    let first = run_sync(&SlackProvider, &context, SyncReason::Manual)
        .await
        .unwrap();
    assert_eq!(first.batch.records.len(), 1);
    assert!(
        !first.batch.complete,
        "a run stopped by the limit has more to give"
    );

    let second = run_sync(&SlackProvider, &context, SyncReason::Scheduled)
        .await
        .unwrap();
    assert_eq!(second.batch.records.len(), 1);
    assert_ne!(
        second.batch.records[0].item_id, first.batch.records[0].item_id,
        "the resumed run must not re-read the message the first one took"
    );
}

#[tokio::test]
async fn a_second_full_walk_re_reads_as_skips_rather_than_duplicates() {
    // A completed walk clears the cursor, so the next one starts at the top.
    // The seen-set is what stops that from filling memory with copies.
    let actions = Arc::new(ScriptedActions::default());
    for _ in 0..2 {
        actions.queue(
            "SLACK_LIST_CONVERSATIONS",
            Ok(channels(&[("C1", "eng"), ("C2", "ops")], "")),
        );
    }
    for _ in 0..2 {
        actions.queue(
            "SLACK_FETCH_CONVERSATION_HISTORY",
            Ok(history(&[message("1700000001.000100", "one")], "")),
        );
        actions.queue(
            "SLACK_FETCH_CONVERSATION_HISTORY",
            Ok(history(&[message("1700000002.000100", "two")], "")),
        );
    }

    let store = Arc::new(MemoryStore::default());
    let context = context(actions, store);

    let first = run_sync(&SlackProvider, &context, SyncReason::InitialConnect)
        .await
        .unwrap();
    assert_eq!(first.batch.records.len(), 2);
    assert!(first.batch.complete, "the whole workspace was walked");

    let second = run_sync(&SlackProvider, &context, SyncReason::Scheduled)
        .await
        .unwrap();
    assert!(second.batch.records.is_empty(), "nothing new to ingest");
    assert_eq!(
        second.records_skipped, 2,
        "both were recognised, not re-read"
    );
}

#[test]
fn slack_re_syncs_on_the_interval_its_host_has_always_advertised() {
    // Fifteen minutes. The host's own table has named this for `slack` since
    // long before there was a provider to honour it.
    assert_eq!(SlackProvider.sync_interval_secs(), Some(900));
}

// ── threads ─────────────────────────────────────────────────────────

fn parent(ts: &str, text: &str, replies: u64) -> Value {
    json!({ "ts": ts, "user": "U1", "text": text, "reply_count": replies })
}

#[tokio::test]
async fn a_thread_is_drained_before_the_next_page_of_history() {
    // Ingesting the question and not the answer is the failure mode a Slack
    // memory has to avoid, so the detour happens before history continues.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[parent("1700000001.000100", "question", 2)],
            "hist2",
        )),
    );
    actions.queue(
        "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION",
        Ok(history(
            &[
                // Slack repeats the parent at the head of its own thread.
                parent("1700000001.000100", "question", 2),
                message("1700000001.000200", "answer"),
            ],
            "",
        )),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000000.000100", "older")], "")),
    );

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));
    let seen = drain(&context).await;

    assert_eq!(
        seen,
        vec![
            "C1:1700000001.000100",
            "C1:1700000001.000200",
            "C1:1700000000.000100"
        ],
        "the reply must land before the next page of history"
    );
    let asked = actions.calls_to("SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION");
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["ts"], json!("1700000001.000100"));
    assert_eq!(asked[0]["channel"], json!("C1"));
}

#[tokio::test]
async fn a_reply_says_it_is_one() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[parent("1700000001.000100", "question", 1)], "")),
    );
    actions.queue(
        "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION",
        Ok(history(
            &[
                parent("1700000001.000100", "question", 1),
                message("1700000001.000200", "answer"),
            ],
            "",
        )),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let provider = SlackProvider;

    let first = provider.fetch_page(&context, None).await.unwrap();
    assert_eq!(first.records[0].title, "#eng — U1");

    let thread = provider
        .fetch_page(&context, first.next_cursor.as_deref())
        .await
        .unwrap();
    assert_eq!(thread.records.len(), 1, "the parent is not ingested twice");
    assert_eq!(thread.records[0].title, "#eng — U1 (reply)");
}

#[tokio::test]
async fn a_thread_that_spans_pages_is_followed_to_its_end() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[parent("1700000001.000100", "question", 2)], "")),
    );
    actions.queue(
        "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION",
        Ok(history(
            &[message("1700000001.000200", "first")],
            "replies2",
        )),
    );
    actions.queue(
        "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION",
        Ok(history(&[message("1700000001.000300", "second")], "")),
    );

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));
    let seen = drain(&context).await;

    assert_eq!(
        seen,
        vec![
            "C1:1700000001.000100",
            "C1:1700000001.000200",
            "C1:1700000001.000300"
        ]
    );
    let asked = actions.calls_to("SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION");
    assert_eq!(asked[1]["cursor"], json!("replies2"));
}

#[tokio::test]
async fn an_unreadable_thread_costs_its_replies_not_the_run() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[parent("1700000001.000100", "question", 2)],
            "hist2",
        )),
    );
    actions.queue(
        "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION",
        Err(Error::Action {
            action: "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION".into(),
            message: "thread_not_found".into(),
        }),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000000.000100", "older")], "")),
    );

    let context = context(actions, Arc::new(MemoryStore::default()));
    let seen = drain(&context).await;

    assert_eq!(
        seen,
        vec!["C1:1700000001.000100", "C1:1700000000.000100"],
        "history carries on where the thread failed"
    );
}

#[tokio::test]
async fn a_channel_is_not_finished_until_its_threads_are() {
    // The high-water mark is what a later run reads from. Promoting it while
    // replies are still owed would strand them above the mark for ever.
    let actions = Arc::new(ScriptedActions::default());
    for _ in 0..2 {
        actions.queue(
            "SLACK_LIST_CONVERSATIONS",
            Ok(channels(&[("C1", "eng")], "")),
        );
    }
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[parent("1700000009.000100", "question", 1)], "")),
    );
    actions.queue(
        "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION",
        Ok(history(&[message("1700000009.000200", "answer")], "")),
    );
    actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));
    let provider = SlackProvider;

    // Walk one: history page, then the thread.
    let first = provider.fetch_page(&context, None).await.unwrap();
    let cursor = first.next_cursor.expect("the thread is owed");
    provider.fetch_page(&context, Some(&cursor)).await.unwrap();

    // Walk two: the mark was promoted only once the thread was drained.
    provider.fetch_page(&context, None).await.unwrap();
    let asked = actions.calls_to("SLACK_FETCH_CONVERSATION_HISTORY");
    assert_eq!(asked[1]["oldest"], json!("1700000009.000100"));
}

#[tokio::test]
async fn the_directory_is_read_across_pages() {
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_LIST_ALL_USERS",
        Ok(json!({ "data": {
            "members": [{ "id": "U1", "real_name": "Ada" }],
            "response_metadata": { "next_cursor": "users2" }
        } })),
    );
    actions.queue(
        "SLACK_LIST_ALL_USERS",
        Ok(json!({ "data": { "members": [{ "id": "U2", "real_name": "Grace" }] } })),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[json!({ "ts": "1700000000.000100", "user": "U2", "text": "hi <@U1>" })],
            "",
        )),
    );

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));
    let page = SlackProvider.fetch_page(&context, None).await.unwrap();

    assert_eq!(actions.calls_to("SLACK_LIST_ALL_USERS").len(), 2);
    assert_eq!(
        page.records[0].title, "#eng — Grace",
        "a name from the second page still resolves"
    );
    assert_eq!(page.records[0].content, "hi @Ada");
}

#[tokio::test]
async fn a_wider_depth_window_reads_past_a_mark_collected_under_a_narrower_one() {
    // The host forwards a per-source "sync depth" into the run's limits. Once a
    // channel is marked, honouring the mark unconditionally would mean widening
    // that setting returns nothing older and says so nowhere.
    let actions = Arc::new(ScriptedActions::default());
    for _ in 0..3 {
        actions.queue(
            "SLACK_LIST_CONVERSATIONS",
            Ok(channels(&[("C1", "eng")], "")),
        );
    }
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000009.000100", "seen")], "")),
    );
    for _ in 0..2 {
        actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));
    }

    let store = Arc::new(MemoryStore::default());
    let mut context = context(actions.clone(), store);
    let provider = SlackProvider;

    // A fortnight-deep walk marks the channel.
    context.limits = SyncLimits {
        max_items: 50,
        depth_days: Some(14),
    };
    provider.fetch_page(&context, None).await.unwrap();

    // Ninety days is wider than the mark speaks for, so the mark stands aside.
    context.limits = SyncLimits {
        max_items: 50,
        depth_days: Some(90),
    };
    provider.fetch_page(&context, None).await.unwrap();

    // A week is inside it, so the mark still saves the re-read.
    context.limits = SyncLimits {
        max_items: 50,
        depth_days: Some(7),
    };
    provider.fetch_page(&context, None).await.unwrap();

    let asked = actions.calls_to("SLACK_FETCH_CONVERSATION_HISTORY");
    assert_ne!(
        asked[1]["oldest"],
        json!("1700000009.000100"),
        "a wider window must read past the mark"
    );
    assert_eq!(
        asked[2]["oldest"],
        json!("1700000009.000100"),
        "a narrower window is already covered by it"
    );
}

#[tokio::test]
async fn an_unbounded_run_reads_past_a_bounded_mark() {
    let actions = Arc::new(ScriptedActions::default());
    for _ in 0..2 {
        actions.queue(
            "SLACK_LIST_CONVERSATIONS",
            Ok(channels(&[("C1", "eng")], "")),
        );
    }
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(&[message("1700000009.000100", "seen")], "")),
    );
    actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));

    let store = Arc::new(MemoryStore::default());
    let mut context = context(actions.clone(), store);
    context.limits = SyncLimits {
        max_items: 50,
        depth_days: Some(30),
    };
    SlackProvider.fetch_page(&context, None).await.unwrap();

    context.limits = SyncLimits {
        max_items: 50,
        depth_days: None,
    };
    SlackProvider.fetch_page(&context, None).await.unwrap();

    let asked = actions.calls_to("SLACK_FETCH_CONVERSATION_HISTORY");
    assert!(
        asked[1].get("oldest").is_none(),
        "no bound is wider than any bound: {}",
        asked[1]
    );
}

#[tokio::test]
async fn a_file_share_survives_the_walk_and_reaches_the_page() {
    // The parse tests prove a file becomes a record. This proves the record
    // actually comes out of `fetch_page` — the wiring between the two is the
    // part a payload test cannot see.
    let actions = Arc::new(ScriptedActions::default());
    actions.queue(
        "SLACK_LIST_CONVERSATIONS",
        Ok(channels(&[("C1", "eng")], "")),
    );
    actions.queue(
        "SLACK_LIST_ALL_USERS",
        Ok(json!({ "data": { "members": [
        { "id": "U1", "profile": { "display_name": "Ada" } }
    ] } })),
    );
    actions.queue(
        "SLACK_FETCH_CONVERSATION_HISTORY",
        Ok(history(
            &[json!({
                "ts": "1700000000.000100",
                "user": "U1",
                "text": "",
                "files": [{
                    "id": "F1",
                    "name": "screenshot.png",
                    "mimetype": "image/png",
                    "permalink": "https://slack.com/files/F1"
                }]
            })],
            "",
        )),
    );
    actions.queue("SLACK_FETCH_CONVERSATION_HISTORY", Ok(history(&[], "")));

    let context = context(actions.clone(), Arc::new(MemoryStore::default()));

    let page = SlackProvider.fetch_page(&context, None).await.unwrap();

    assert_eq!(
        page.records.len(),
        1,
        "the share carried no comment, so the file is the only record"
    );
    assert_eq!(page.records[0].item_id, "C1:1700000000.000100:F1");
    assert_eq!(page.records[0].title, "#eng — Ada shared screenshot.png");
    assert_eq!(
        page.records[0].url.as_deref(),
        Some("https://slack.com/files/F1")
    );
}
