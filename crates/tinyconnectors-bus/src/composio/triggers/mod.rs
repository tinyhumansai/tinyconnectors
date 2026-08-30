//! The trigger catalog, active subscriptions, and the events they deliver.
//!
//! A trigger is a webhook subscription on a connected account. Three
//! distinct things live here and the naming is worth reading once:
//!
//! - [`ComposioAvailableTrigger`] — a trigger a user *could* enable. Some are
//!   global to the toolkit (`scope: "static"`), some are per repository
//!   (`scope: "github_repo"`).
//! - [`ComposioActiveTrigger`] — a subscription that *is* enabled.
//! - [`ComposioTriggerEvent`] — one delivery. The backend HMAC-verifies the
//!   webhook and fans it out over the user's sockets; this is that payload.
//!
//! [`ComposioTriggerHistoryEntry`] is the module's own record of a delivery,
//! not a backend shape: it is what a daily JSONL archive holds so a user can
//! see what arrived and when.
//!
//! # Why several fields tolerate objects
//!
//! [`ComposioActiveTrigger`] decodes `id`, `slug`, `toolkit`, `connectionId`
//! and `state` through the drift-tolerant deserializers in
//! `super::serde_compat`. Composio has turned each of these from a bare string
//! into `{"slug": …, "logo": …}` at least once, and a strict `String` field
//! turns that into an empty trigger list — which reads to a user as their
//! subscriptions having been silently deleted.

mod types;

pub use types::{
    ComposioActiveTrigger, ComposioActiveTriggersResponse, ComposioAvailableTrigger,
    ComposioAvailableTriggerRepo, ComposioAvailableTriggersResponse, ComposioCreateTriggerResponse,
    ComposioDisableTriggerResponse, ComposioEnableTriggerResponse, ComposioTriggerEvent,
    ComposioTriggerHistoryEntry, ComposioTriggerHistoryResult, ComposioTriggerMetadata,
};

#[cfg(test)]
mod test;
