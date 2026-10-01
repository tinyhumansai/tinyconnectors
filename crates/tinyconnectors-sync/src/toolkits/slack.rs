//! The Slack provider.
//!
//! # Why this one is not a [`PageSpec`](crate::pipeline::PageSpec)
//!
//! Every other toolkit here is *flat*: one action reads the whole account, one
//! cursor says where it got to, and the provider is a declaration. Slack is
//! *scoped*. `SLACK_FETCH_CONVERSATION_HISTORY` reads one channel and requires
//! its id, so a sync has to walk the conversation list and page each channel in
//! turn, remembering a position per channel. No single action reads a
//! workspace.
//!
//! It is scoped twice over. A channel's history carries only the top of each
//! conversation; the replies under a message are a second read, against a
//! second action, with a cursor of their own. Ingesting the question and not
//! the answer is the failure mode a Slack memory has to avoid, so a walk
//! detours into a message's thread before it reads the next page of history.
//!
//! # How that fits a pipeline built for one cursor
//!
//! [`run_sync`](crate::pipeline::run_sync) never looks inside a cursor — it
//! stores the string a page reports and hands it back on the next call. So the
//! three-level walk fits without changing the pipeline at all: the cursor
//! carries `"<index>|<history>|<thread>|<thread cursor>"`, and each call
//! returns exactly one page of one channel or one thread.
//!
//! That is what keeps the rule in [`ConnectorProvider::fetch_page`] intact. The
//! item limit, the daily request budget and the already-ingested set stay where
//! they are, in the run loop, instead of being re-implemented here so that one
//! toolkit could loop internally.
//!
//! # What lives beside the cursor, and why it is not in it
//!
//! The channel roster, the queue of threads owed by the channel being read, the
//! per-channel high-water marks and the user directory are kept in this
//! provider's own key in the host's state store, not encoded in the cursor — a
//! cursor is a position, and a roster of two hundred channels is not one.
//!
//! It has to be a **different key** from the one
//! [`SyncState`](crate::state::SyncState) uses. The run loop loads that state
//! when a run starts and writes it back when the run ends, so anything this
//! provider wrote to the same key mid-run would be overwritten by a copy that
//! predates it.
//!
//! # Reading a channel twice is cheaper than reading it from the start
//!
//! A completed walk clears the cursor, so the next run begins again at the
//! first channel. Without a lower bound that would re-read every channel back
//! to the depth boundary on every scheduled run and discard all of it as
//! already seen. Each channel therefore records the newest message the walk
//! has finished reading, and later runs ask Slack only for what is newer.
//!
//! The mark advances when a channel is **exhausted**, never when a page of it
//! is read: a walk stopped half way through a channel by the item limit must
//! resume where it stopped, and a mark moved early would step over everything
//! below it. A channel is not exhausted until the threads it owes are drained,
//! so a reply is never stranded above the mark.
//!
//! # What this deliberately does not read
//!
//! Direct messages and group DMs. The conversation list asks for
//! `public_channel,private_channel` only. A Slack connection is granted for a
//! workspace, and quietly ingesting someone's private correspondence into a
//! memory the agent quotes from is not what connecting a workspace asks for.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tinyconnectors_bus::records::ConnectorRecord;

use super::identity::pick;
use super::slack_catalog::CURATED;
use super::slack_parse::{has_replies, is_fatal, messages_in, next_cursor, records_from};
use crate::pipeline::{ProviderPage, first_array, pick_str};
use crate::provider::{ConnectorProvider, ProviderContext, ProviderUserProfile};
use crate::scope::CuratedTool;
use crate::state::STATE_NAMESPACE;
use crate::{Error, Result};

/// Lists the conversations the connected account can read.
const ACTION_CONVERSATIONS: &str = "SLACK_LIST_CONVERSATIONS";
/// Reads one page of one conversation.
const ACTION_HISTORY: &str = "SLACK_FETCH_CONVERSATION_HISTORY";
/// Reads one page of the replies under one message.
const ACTION_THREAD: &str = "SLACK_FETCH_MESSAGE_THREAD_FROM_A_CONVERSATION";
/// Reads the workspace the connection belongs to.
const PROFILE_ACTION: &str = "SLACK_FETCH_TEAM_INFO";
/// Reads the directory used to turn `<@U…>` into a name.
const ACTION_USERS: &str = "SLACK_LIST_ALL_USERS";

/// Conversations asked for per roster page.
///
/// Slack's own maximum for this call. The roster is walked one page at a time
/// like everything else, so a large workspace costs one extra request per two
/// hundred channels rather than a loop inside a single page read.
const CHANNELS_PER_PAGE: usize = 200;

/// Ceiling on messages asked for in one history or thread page.
///
/// The run's own item limit is the real bound and is almost always smaller;
/// this only stops an unbounded limit from asking Slack for more than it will
/// return anyway.
const MESSAGES_PER_PAGE: usize = 200;

/// Directory entries asked for per page.
const USERS_PER_PAGE: usize = 200;

/// Most directory pages read in one walk.
///
/// The directory is a nicety — it turns `<@U04AB>` into a name — so it is
/// bounded rather than walked to the end. A workspace larger than
/// `USERS_PER_PAGE * USER_PAGES_MAX` people resolves the first thousand and
/// leaves the rest as ids, which reads worse but costs nothing else. Spending
/// an unbounded number of requests on it before a single message is read would
/// be the wrong trade.
const USER_PAGES_MAX: usize = 5;

/// How long a cached user directory is trusted, in milliseconds.
///
/// A day. Display names change rarely, and the cost of a stale one is a name
/// that reads slightly wrong in a message body — far below the cost of another
/// directory fetch at the head of every walk.
const USERS_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// Where the walk is: which channel, where in its history, and which of its
/// threads is being drained.
///
/// Encoded as `"<index>|<history>|<thread>|<thread cursor>"`. No field can
/// contain a `|`: the index is a number, a Slack `ts` is digits and a dot, and
/// Slack's cursors are base64, whose alphabet does not include one.
///
/// `history` keeps its meaning while a thread is being read — it is where the
/// channel resumes once the detour ends.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Cursor {
    /// Position in the roster page held in [`WalkState::channels`].
    index: usize,
    /// Slack's own position inside that channel's history, if part way in.
    history: Option<String>,
    /// The message whose replies are being read, if the walk is in a thread.
    thread: Option<String>,
    /// Slack's position inside those replies, if part way in.
    reply_page: Option<String>,
}

impl Cursor {
    /// A cursor naming `index`, at `history` within it, reading no thread.
    fn new(index: usize, history: Option<String>) -> Self {
        Self {
            index,
            history,
            thread: None,
            reply_page: None,
        }
    }

    /// A cursor part way through the replies under `thread`.
    fn in_thread(
        index: usize,
        history: Option<String>,
        thread: String,
        reply_page: Option<String>,
    ) -> Self {
        Self {
            index,
            history,
            thread: Some(thread),
            reply_page,
        }
    }

    /// Read a cursor the pipeline handed back.
    ///
    /// A cursor that does not parse restarts the walk rather than failing it:
    /// the seen-set turns the re-read into skips, so the cost of being wrong
    /// here is requests, and the cost of erroring would be a connection that
    /// never syncs again. A cursor written by an older release has fewer
    /// fields and reads as "not in a thread", which is exactly right.
    fn decode(raw: Option<&str>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        let mut fields = raw.splitn(4, '|');
        // A position that does not parse names no channel — and a history or
        // thread cursor beside it belongs to a channel nobody can now identify.
        // Keeping them would ask Slack to resume channel zero from another
        // channel's page, which it refuses, costing that channel the walk.
        let Ok(index) = fields.next().unwrap_or_default().parse() else {
            return Self::default();
        };
        let owned = |field: Option<&str>| {
            field
                .filter(|value| !value.is_empty())
                .map(std::string::ToString::to_string)
        };
        Self {
            index,
            history: owned(fields.next()),
            thread: owned(fields.next()),
            reply_page: owned(fields.next()),
        }
    }

    /// Render this position for the pipeline to store.
    fn encode(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.index,
            self.history.as_deref().unwrap_or(""),
            self.thread.as_deref().unwrap_or(""),
            self.reply_page.as_deref().unwrap_or("")
        )
    }
}

/// One conversation the walk knows about.
///
/// Reachable from [`super::slack_parse`], which needs the channel a record
/// belongs to in order to give it a channel-qualified id and title.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Channel {
    /// Slack channel id, e.g. `C0123ABCD`.
    pub(super) id: String,
    /// Channel name without the leading `#`.
    #[serde(default)]
    pub(super) name: String,
}

/// What the walk remembers between pages and between runs.
///
/// Persisted under this provider's own key — see the module docs for why it
/// cannot share the run loop's.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WalkState {
    /// The roster page being walked.
    #[serde(default)]
    channels: Vec<Channel>,
    /// Where the next roster page starts, when there is one.
    #[serde(default)]
    channels_cursor: Option<String>,
    /// Messages in the channel being read whose replies are still owed.
    #[serde(default)]
    threads: Vec<String>,
    /// Newest message fully read per channel, `channel id → mark`.
    #[serde(default)]
    high_water: BTreeMap<String, Mark>,
    /// Newest message id *seen* per channel, promoted once the channel ends.
    #[serde(default)]
    pending: BTreeMap<String, String>,
    /// `user id → display name`, for resolving mentions and authors.
    #[serde(default)]
    users: BTreeMap<String, String>,
    /// When the directory was read, in milliseconds since the Unix epoch.
    #[serde(default)]
    users_fetched_ms: i64,
}

impl WalkState {
    /// The store key holding one connection's walk.
    ///
    /// Deliberately unlike [`crate::state::SyncState::key`], which is
    /// `"<toolkit>:<connection>"`.
    fn key(connection_id: &str) -> String {
        format!("slack-walk:{connection_id}")
    }

    /// Load this connection's walk, or an empty one.
    async fn load(context: &ProviderContext) -> Result<Self> {
        let key = Self::key(&context.connection_id);
        let Some(value) = context.state.get(STATE_NAMESPACE, &key).await? else {
            return Ok(Self::default());
        };
        // A shape that no longer decodes restarts the walk instead of failing
        // it, for the same reason a bad cursor does.
        Ok(serde_json::from_value(value).unwrap_or_default())
    }

    /// Persist this walk.
    async fn save(&self, context: &ProviderContext) -> Result<()> {
        let key = Self::key(&context.connection_id);
        let value = serde_json::to_value(self).map_err(|error| Error::Decode {
            key: key.clone(),
            message: error.to_string(),
        })?;
        context.state.set(STATE_NAMESPACE, &key, &value).await
    }

    /// The channel at `index` of the roster page in hand.
    fn channel_at(&self, index: usize) -> Option<&Channel> {
        self.channels.get(index)
    }

    /// Where to start reading `channel`, honouring the run's depth bound.
    ///
    /// The high-water mark wins when there is one: it is never older than the
    /// bound, having been set by a run that already respected it.
    fn oldest_for(&self, channel: &str, depth_days: Option<u32>) -> Option<String> {
        if let Some(mark) = self.high_water.get(channel)
            && mark.covers(depth_days)
        {
            return Some(mark.ts.clone());
        }
        depth_days.map(|days| {
            let since = Utc::now() - TimeDelta::days(i64::from(days));
            format!("{}.000000", since.timestamp())
        })
    }

    /// Promote the pending mark for a channel that has just been exhausted.
    ///
    /// `depth_days` is the window the run read under, and is stored with the
    /// position: a later, wider run has to be able to tell that this mark does
    /// not speak for the ground it wants to cover.
    fn commit_high_water(&mut self, channel: &str, depth_days: Option<u32>) {
        if let Some(ts) = self.pending.remove(channel) {
            self.high_water
                .insert(channel.to_string(), Mark { ts, depth_days });
        }
    }

    /// Whether the user directory needs re-reading.
    fn users_are_stale(&self, now_ms: i64) -> bool {
        self.users.is_empty() || now_ms - self.users_fetched_ms > USERS_TTL_MS
    }
}

/// How far a channel has been read, and under what window.
///
/// The window matters as much as the position. A mark collected under a
/// fourteen-day bound says nothing about the ninety-first day, so a later run
/// asking for more must be allowed past it — otherwise widening the setting
/// silently returns nothing older and says so nowhere.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Mark {
    /// Newest message id read to completion, as a Slack `ts`.
    ts: String,
    /// Days the run that set it was bounded to; `None` was unbounded.
    #[serde(default)]
    depth_days: Option<u32>,
}

impl Mark {
    /// Whether this mark already covers a run bounded to `requested`.
    ///
    /// An unbounded mark covers everything. An unbounded *request* is wider
    /// than any bounded mark, so nothing bounded covers it.
    fn covers(&self, requested: Option<u32>) -> bool {
        match (self.depth_days, requested) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(collected), Some(requested)) => requested <= collected,
        }
    }
}

/// One page read, and whether it changed anything worth persisting.
struct Read {
    /// What to hand back to the run loop.
    page: ProviderPage,
    /// Whether [`WalkState`] moved and needs saving.
    changed: bool,
}

/// Slack as a connector toolkit.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlackProvider;

impl SlackProvider {
    /// Read one roster page, replacing the one in hand.
    async fn load_channel_page(
        &self,
        context: &ProviderContext,
        walk: &mut WalkState,
        cursor: Option<&str>,
    ) -> Result<()> {
        let mut arguments = json!({
            "limit": CHANNELS_PER_PAGE,
            // Channels only. See the module docs: DMs are deliberately not read.
            "types": "public_channel,private_channel",
            "exclude_archived": true,
        });
        if let Some(cursor) = cursor {
            arguments["cursor"] = Value::String(cursor.to_string());
        }

        let payload = context.run(ACTION_CONVERSATIONS, arguments).await?;
        walk.channels = first_array(
            &payload,
            &["/data/channels", "/channels", "/data/data/channels"],
        )
        .iter()
        .filter_map(|channel| {
            let id = pick_str(channel, &["id", "data.id"])?;
            let name = pick_str(channel, &["name", "data.name"]).unwrap_or_else(|| id.clone());
            Some(Channel { id, name })
        })
        .collect();
        // Slack handing back the cursor it was just given would walk the same
        // page for ever: the run loop stops on records or on the daily budget,
        // and a roster page that yields no channels produces neither. Refusing
        // a cursor that has not moved ends the walk instead of spending the
        // budget discovering the same nothing.
        walk.channels_cursor = next_cursor(&payload).filter(|next| Some(next.as_str()) != cursor);
        Ok(())
    }

    /// Refresh the user directory, if it is missing or old.
    ///
    /// A failure here is not a failure of the walk: mentions render as raw ids,
    /// which is worse to read and no reason to ingest nothing. Bounded by
    /// [`USER_PAGES_MAX`] — see there for why it does not read to the end.
    async fn refresh_users(&self, context: &ProviderContext, walk: &mut WalkState) -> bool {
        let now_ms = Utc::now().timestamp_millis();
        if !walk.users_are_stale(now_ms) {
            return false;
        }

        let mut directory = BTreeMap::new();
        let mut cursor: Option<String> = None;
        for _ in 0..USER_PAGES_MAX {
            let mut arguments = json!({ "limit": USERS_PER_PAGE });
            if let Some(cursor) = cursor.as_deref() {
                arguments["cursor"] = Value::String(cursor.to_string());
            }
            let Ok(payload) = context.run(ACTION_USERS, arguments).await else {
                tracing::debug!("[connectors][slack] user directory unavailable; ids stay raw");
                break;
            };
            for user in &first_array(
                &payload,
                &["/data/members", "/members", "/data/data/members"],
            ) {
                if let Some(id) = pick_str(user, &["id"])
                    && let Some(name) =
                        pick_str(user, &["profile.display_name", "real_name", "name"])
                {
                    directory.insert(id, name);
                }
            }
            cursor = next_cursor(&payload).filter(|next| Some(next.as_str()) != cursor.as_deref());
            if cursor.is_none() {
                break;
            }
        }

        if directory.is_empty() {
            return false;
        }
        walk.users = directory;
        walk.users_fetched_ms = now_ms;
        true
    }

    /// Move to the next roster page, or report the workspace walked.
    async fn advance_roster(
        &self,
        context: &ProviderContext,
        walk: &mut WalkState,
    ) -> Result<Read> {
        let Some(next) = walk.channels_cursor.clone() else {
            return Ok(Read {
                page: empty_page(None),
                changed: false,
            });
        };
        self.load_channel_page(context, walk, Some(&next)).await?;
        Ok(Read {
            page: empty_page(Some(Cursor::new(0, None).encode())),
            changed: true,
        })
    }

    /// Read one page of the replies under the message the cursor names.
    async fn read_thread(
        &self,
        context: &ProviderContext,
        walk: &mut WalkState,
        position: &Cursor,
        channel: &Channel,
        parent: &str,
    ) -> Result<Read> {
        let mut arguments = json!({
            "channel": channel.id,
            "ts": parent,
            "limit": MESSAGES_PER_PAGE,
        });
        if let Some(cursor) = position.reply_page.as_deref() {
            arguments["cursor"] = Value::String(cursor.to_string());
        }

        let payload = match context.run(ACTION_THREAD, arguments).await {
            Ok(payload) => payload,
            Err(error) if is_fatal(&error) => return Err(error),
            Err(error) => {
                // A thread that cannot be read costs its replies, not the run.
                tracing::debug!(
                    channel = %channel.id,
                    thread = %parent,
                    error = %error,
                    "[connectors][slack] thread skipped"
                );
                return Ok(after_thread(
                    walk,
                    position,
                    channel,
                    parent,
                    context.limits.depth_days,
                    Vec::new(),
                    None,
                ));
            }
        };

        let replies = messages_in(&payload);
        // The parent is repeated as the first reply. It was ingested with the
        // history page that found it, and the run loop would skip it anyway;
        // dropping it here saves the round trip through the seen-set.
        let (records, versions) = records_from(&replies, channel, &walk.users, Some(parent));
        let more = next_cursor(&payload);
        let mut read = after_thread(
            walk,
            position,
            channel,
            parent,
            context.limits.depth_days,
            records,
            more,
        );
        read.page.versions = versions;
        Ok(read)
    }

    /// Read one page of the channel the cursor names.
    async fn read_history(
        &self,
        context: &ProviderContext,
        walk: &mut WalkState,
        position: &Cursor,
        channel: &Channel,
    ) -> Result<Read> {
        let mut arguments = json!({
            "channel": channel.id,
            "inclusive": false,
            "limit": context.limits.max_items.clamp(1, MESSAGES_PER_PAGE),
        });
        if let Some(oldest) = walk.oldest_for(&channel.id, context.limits.depth_days) {
            arguments["oldest"] = Value::String(oldest);
        }
        if let Some(history) = position.history.as_deref() {
            arguments["cursor"] = Value::String(history.to_string());
        }

        let payload = match context.run(ACTION_HISTORY, arguments).await {
            Ok(payload) => payload,
            Err(error) if is_fatal(&error) => return Err(error),
            Err(error) => {
                // One channel the account cannot read is ordinary — archived,
                // not a member, rate-limited. Skipping it costs that channel;
                // failing here would cost the workspace.
                tracing::debug!(
                    channel = %channel.id,
                    error = %error,
                    "[connectors][slack] channel skipped"
                );
                walk.threads.clear();
                return Ok(Read {
                    page: empty_page(Some(Cursor::new(position.index + 1, None).encode())),
                    changed: true,
                });
            }
        };

        let messages = messages_in(&payload);

        // Entering a channel: nothing earlier owes it replies, and a queue left
        // behind by an interrupted walk of another channel must not be drained
        // against this one.
        if position.history.is_none() {
            walk.threads.clear();
            // Slack returns a channel newest-first, so the newest message of
            // the whole channel is on its first page. Remember it now and
            // promote it only when the channel ends — see the module docs.
            if let Some(newest) = messages.iter().find_map(|m| pick_str(m, &["ts"])) {
                walk.pending.insert(channel.id.clone(), newest);
            }
        }

        // Queue the conversations under this page before reading further: they
        // belong to messages this page just ingested. Pushed back-to-front
        // because the queue is drained from its end, which makes the threads
        // come back in the order Slack listed their parents.
        walk.threads
            .extend(messages.iter().rev().filter_map(has_replies));

        let (records, versions) = records_from(&messages, channel, &walk.users, None);
        let history = next_cursor(&payload);
        let next = if let Some(parent) = walk.threads.pop() {
            Cursor::in_thread(position.index, history, parent, None)
        } else if history.is_some() {
            Cursor::new(position.index, history)
        } else {
            // Nothing more in this channel and nothing owed: promote the mark
            // it has been collecting, and move on.
            walk.commit_high_water(&channel.id, context.limits.depth_days);
            Cursor::new(position.index + 1, None)
        };

        Ok(Read {
            page: ProviderPage {
                records,
                versions,
                next_cursor: Some(next.encode()),
                ..ProviderPage::default()
            },
            changed: true,
        })
    }
}

#[async_trait]
impl ConnectorProvider for SlackProvider {
    fn toolkit_slug(&self) -> &'static str {
        "slack"
    }

    fn description(&self) -> &'static str {
        "Read and send Slack messages, and ingest channel history as memory."
    }

    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        Some(CURATED)
    }

    fn sync_interval_secs(&self) -> Option<u64> {
        Some(900)
    }

    async fn fetch_user_profile(&self, context: &ProviderContext) -> Result<ProviderUserProfile> {
        let payload = context.run(PROFILE_ACTION, json!({})).await?;
        Ok(ProviderUserProfile {
            toolkit: self.toolkit_slug().to_string(),
            connection_id: Some(context.connection_id.clone()),
            // The workspace is what a Slack connection is *of*; a UI labelling
            // an account has nothing better to show.
            display_name: pick(&payload, &["team.name", "name", "data.team.name"]),
            username: pick(&payload, &["team.domain", "domain", "data.team.domain"]),
            avatar_url: pick(&payload, &["team.icon.image_132", "icon.image_132"]),
            extras: payload,
            ..ProviderUserProfile::default()
        })
    }

    /// Read one page of one channel, or of one thread, advancing the walk.
    ///
    /// Every return either moves the cursor forward or reports the walk is
    /// over. That is what keeps the run loop from spinning: it stops only on a
    /// limit or on `None`, so a page that reported the position it was given
    /// would be asked for again forever.
    async fn fetch_page(
        &self,
        context: &ProviderContext,
        cursor: Option<&str>,
    ) -> Result<ProviderPage> {
        let position = Cursor::decode(cursor);
        let mut walk = WalkState::load(context).await?;
        let mut changed = false;

        // A run with no cursor is a walk starting: re-read the roster from the
        // top, and refresh the directory it renders names with.
        if cursor.is_none() {
            self.load_channel_page(context, &mut walk, None).await?;
            self.refresh_users(context, &mut walk).await;
            changed = true;
        }

        let read = match walk.channel_at(position.index).cloned() {
            None => self.advance_roster(context, &mut walk).await?,
            Some(channel) => match position.thread.clone() {
                Some(parent) => {
                    self.read_thread(context, &mut walk, &position, &channel, &parent)
                        .await?
                }
                None => {
                    self.read_history(context, &mut walk, &position, &channel)
                        .await?
                }
            },
        };

        if changed || read.changed {
            walk.save(context).await?;
        }
        Ok(read.page)
    }
}

/// Where the walk goes once a thread page has been read.
fn after_thread(
    walk: &mut WalkState,
    position: &Cursor,
    channel: &Channel,
    parent: &str,
    depth_days: Option<u32>,
    records: Vec<ConnectorRecord>,
    more: Option<String>,
) -> Read {
    let history = position.history.clone();
    let next = if let Some(more) = more {
        // Same thread, further in. Named from the argument rather than read
        // back out of the cursor: a `None` there would silently report the
        // whole workspace walked and abandon every channel after this one.
        Some(Cursor::in_thread(
            position.index,
            history,
            parent.to_string(),
            Some(more),
        ))
    } else if let Some(parent) = walk.threads.pop() {
        // This channel still owes replies elsewhere.
        Some(Cursor::in_thread(position.index, history, parent, None))
    } else if history.is_some() {
        // Threads drained; carry on where the history left off.
        Some(Cursor::new(position.index, history))
    } else {
        // Nothing left in this channel, replies included: the mark is safe.
        walk.commit_high_water(&channel.id, depth_days);
        Some(Cursor::new(position.index + 1, None))
    };

    Read {
        page: ProviderPage {
            records,
            versions: Vec::new(),
            next_cursor: next.map(|cursor| cursor.encode()),
            ..ProviderPage::default()
        },
        changed: true,
    }
}

/// A page carrying nothing, resuming at `next`.
fn empty_page(next: Option<String>) -> ProviderPage {
    ProviderPage {
        next_cursor: next,
        ..ProviderPage::default()
    }
}

#[cfg(test)]
#[path = "slack_tests.rs"]
mod test;
