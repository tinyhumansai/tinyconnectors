//! The curated Composio trigger catalog: which upstream events are worth
//! building a personal assistant's features on, and the pure lookups over them.
//!
//! Composio offers hundreds of triggers, and most of them are noise: sync
//! cursors, every edit to every calendar entry, events nobody plans around.
//! Each table here names the few worth subscribing to for one toolkit, keyed by
//! a toolkit-agnostic [`TriggerEvent`] so a host can ask for "new email" without
//! caring which mail toolkit the user connected.
//!
//! Every row carries how Composio delivers it ([`TriggerDelivery`]), how early
//! it is worth building on ([`TriggerTier`]), the configuration keys it takes,
//! and a sentence on what it is good for. A [`TriggerTier::PerResource`]
//! trigger watches one repository, pull request, team or page, so a host has
//! to ask the user which before creating it.
//!
//! The tables are one module per toolkit family, mirroring the curated tool
//! catalogs in [`crate::toolkits::catalog`]. Toolkit names resolve through the
//! same alias folding those catalogs use, so `google_calendar` reaches the
//! `googlecalendar` triggers. `docs/specs/curated-triggers.md` lists every row
//! and records how the list was verified.
//!
//! # Example
//!
//! ```
//! use tinyconnectors_sync::triggers::{TriggerEvent, TriggerTier, trigger_for_event};
//!
//! // The new-email trigger on whatever mail toolkit the user connected.
//! let trigger = trigger_for_event(TriggerEvent::NewEmail, "outlook").expect("curated");
//! assert_eq!(trigger.slug, "OUTLOOK_MESSAGE_TRIGGER");
//! assert_eq!(trigger.tier, TriggerTier::Core);
//! ```

mod github;
mod google;
mod messaging;
mod microsoft;
mod productivity;
mod types;

pub use types::{CuratedTrigger, TriggerDelivery, TriggerEvent, TriggerTier};

use crate::toolkits::catalog::canonical_toolkit;

/// Every curated trigger table, in the order lookups report their rows.
const CURATED_TRIGGERS: &[&[CuratedTrigger]] = &[
    google::GMAIL_TRIGGERS,
    google::GOOGLECALENDAR_TRIGGERS,
    google::GOOGLEDRIVE_TRIGGERS,
    google::GOOGLESUPER_TRIGGERS,
    microsoft::OUTLOOK_TRIGGERS,
    messaging::SLACK_TRIGGERS,
    github::GITHUB_TRIGGERS,
    productivity::LINEAR_TRIGGERS,
    productivity::NOTION_TRIGGERS,
];

/// Every curated trigger, toolkit by toolkit.
pub fn curated_triggers() -> impl Iterator<Item = &'static CuratedTrigger> {
    CURATED_TRIGGERS.iter().flat_map(|table| table.iter())
}

/// The curated trigger named `slug`, matched case-insensitively.
#[must_use]
pub fn curated_trigger(slug: &str) -> Option<&'static CuratedTrigger> {
    let slug = slug.trim();
    curated_triggers().find(|trigger| trigger.slug.eq_ignore_ascii_case(slug))
}

/// The curated triggers of `toolkit`, which may be any spelling the curated
/// tool catalogs accept (`google_calendar`, `GoogleCalendar`, …).
///
/// Empty for a toolkit with no curated triggers.
pub fn curated_triggers_for_toolkit(
    toolkit: &str,
) -> impl Iterator<Item = &'static CuratedTrigger> + use<> {
    let toolkit = canonical_toolkit(toolkit);
    curated_triggers().filter(move |trigger| trigger.toolkit == toolkit)
}

/// The trigger reporting `event` on `toolkit`, if that toolkit has one: the
/// new-email trigger on whatever mail toolkit this user connected.
#[must_use]
pub fn trigger_for_event(event: TriggerEvent, toolkit: &str) -> Option<&'static CuratedTrigger> {
    curated_triggers_for_toolkit(toolkit).find(|trigger| trigger.event == event)
}

/// The toolkits with a trigger reporting `event`, each once, in table order.
pub fn toolkits_for_event(event: TriggerEvent) -> impl Iterator<Item = &'static str> {
    // A toolkit has at most one trigger per event, which the tests pin, so
    // filtering by event alone lists each toolkit once.
    curated_triggers()
        .filter(move |trigger| trigger.event == event)
        .map(|trigger| trigger.toolkit)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
