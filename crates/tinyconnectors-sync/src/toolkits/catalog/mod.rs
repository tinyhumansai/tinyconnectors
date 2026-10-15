//! The curated Composio tool catalogs, one module per toolkit family, and the
//! pure toolkit-to-catalog lookup over them.
//!
//! Composio publishes 60+ actions per toolkit; most are noise for an agent's
//! planning loop. Each toolkit gets a hand-curated `&'static [CuratedTool]`
//! slice that pares the surface down to a useful subset and tags every action
//! with a [`crate::ToolScope`], so per-user scope preferences can gate
//! execution. The providers in this crate and the module's vocabulary members
//! read the same tables.

pub mod business;
pub mod clickup;
pub mod descriptions;
pub mod github;
pub mod gmail;
pub mod google;
pub mod linear;
pub mod messaging;
pub mod microsoft;
pub mod notion;
pub mod productivity;
pub mod social_media;

use crate::scope::CuratedTool;

pub use descriptions::toolkit_description;

/// Toolkits with a native identity provider (profile facets).
pub const NATIVE_PROVIDERS: &[&str] = &["gmail", "notion", "slack", "clickup", "github", "linear"];

/// Every toolkit with a curated catalog, once each under its canonical slug and
/// in display order. Aliases accepted by [`catalog_for_toolkit`] are not listed.
pub const CURATED_TOOLKITS: &[&str] = &[
    "gmail",
    "notion",
    "slack",
    "clickup",
    "github",
    "discord",
    "googlecalendar",
    "googledrive",
    "googledocs",
    "googlesheets",
    "outlook",
    "microsoft_teams",
    "linear",
    "jira",
    "trello",
    "asana",
    "dropbox",
    "twitter",
    "spotify",
    "telegram",
    "whatsapp",
    "shopify",
    "stripe",
    "hubspot",
    "salesforce",
    "airtable",
    "figma",
    "youtube",
    "one_drive",
    "excel",
    "todoist",
];

/// Does `toolkit` have a native identity provider?
#[must_use]
pub fn has_native_provider(toolkit: &str) -> bool {
    NATIVE_PROVIDERS.contains(&toolkit)
}

/// Static toolkit → curated catalog map.
///
/// The lookup key is the lowercased prefix `toolkit_from_slug` returns for an
/// action slug — `GOOGLECALENDAR_CREATE_EVENT` → `"googlecalendar"`.
/// Multi-segment prefixes like `MICROSOFT_TEAMS_*` return their known toolkit
/// slug. Aliases (`google_calendar`, `onedrive`, …) resolve through
/// [`canonical_toolkit`] first.
#[must_use]
pub fn catalog_for_toolkit(toolkit: &str) -> Option<&'static [CuratedTool]> {
    match canonical_toolkit(toolkit).as_str() {
        // Toolkits with a native provider. Each provider's `curated_tools()`
        // returns this same slice.
        "gmail" => Some(gmail::GMAIL_CURATED),
        "notion" => Some(notion::NOTION_CURATED),
        "github" => Some(github::GITHUB_CURATED),
        "linear" => Some(linear::LINEAR_CURATED),
        "clickup" => Some(clickup::CLICKUP_CURATED),
        "slack" => Some(messaging::SLACK_CURATED),
        // Catalog-only toolkits.
        "discord" => Some(messaging::DISCORD_CURATED),
        "googlecalendar" => Some(google::GOOGLECALENDAR_CURATED),
        "googledrive" => Some(google::GOOGLEDRIVE_CURATED),
        "googledocs" => Some(google::GOOGLEDOCS_CURATED),
        "googlesheets" => Some(google::GOOGLESHEETS_CURATED),
        "outlook" => Some(productivity::OUTLOOK_CURATED),
        "microsoft_teams" => Some(messaging::MICROSOFT_TEAMS_CURATED),
        "jira" => Some(productivity::JIRA_CURATED),
        "trello" => Some(productivity::TRELLO_CURATED),
        "asana" => Some(productivity::ASANA_CURATED),
        "dropbox" => Some(productivity::DROPBOX_CURATED),
        "twitter" => Some(social_media::TWITTER_CURATED),
        "spotify" => Some(social_media::SPOTIFY_CURATED),
        "telegram" => Some(messaging::TELEGRAM_CURATED),
        "whatsapp" => Some(messaging::WHATSAPP_CURATED),
        "shopify" => Some(business::SHOPIFY_CURATED),
        "stripe" => Some(business::STRIPE_CURATED),
        "hubspot" => Some(business::HUBSPOT_CURATED),
        "salesforce" => Some(business::SALESFORCE_CURATED),
        "airtable" => Some(business::AIRTABLE_CURATED),
        "figma" => Some(business::FIGMA_CURATED),
        "youtube" => Some(social_media::YOUTUBE_CURATED),
        "one_drive" => Some(microsoft::ONE_DRIVE_CURATED),
        "excel" => Some(microsoft::EXCEL_CURATED),
        "todoist" => Some(productivity::TODOIST_CURATED),
        _ => None,
    }
}

/// The canonical slug for `toolkit`: trimmed, lowercased, with every alias the
/// catalogs accept folded onto the slug `CURATED_TOOLKITS` lists.
///
/// The trigger catalog resolves toolkits through this too, so an alias that
/// reaches a tool catalog reaches the same toolkit's triggers.
pub(crate) fn canonical_toolkit(toolkit: &str) -> String {
    let lower = toolkit.trim().to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "google_calendar" => "googlecalendar",
        "google_drive" => "googledrive",
        "google_docs" => "googledocs",
        "google_sheets" => "googlesheets",
        // The legacy "microsoft" alias stays while `toolkit_from_slug` returns
        // the precise "microsoft_teams" slug for Teams actions.
        "microsoft" => "microsoft_teams",
        // The multi-segment prefix extracts to "one_drive"; retain "one" as
        // a legacy alias alongside the canonical UI/backend spellings.
        "one" | "onedrive" => "one_drive",
        _ => return lower,
    };
    canonical.to_string()
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
