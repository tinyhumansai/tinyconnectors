//! Compatibility tests for the compiled connector vocabulary.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;

// Tests for the surrounding module.

/// Every toolkit with a curated catalog, in display order.
const CAPABILITY_TOOLKITS: &[&str] = &[
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

#[test]
fn curated_toolkits_are_canonical_and_cover_every_distinct_catalog() {
    let mut distinct: Vec<*const CuratedTool> = Vec::new();
    for toolkit in CURATED_TOOLKITS {
        let catalog = catalog_for_toolkit(toolkit).expect("listed toolkit has a catalog");
        distinct.push(catalog.as_ptr());
    }
    let listed = distinct.len();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        listed,
        distinct.len(),
        "a catalog is listed under two slugs"
    );
    // Every alias the lookup accepts resolves to a listed catalog.
    for alias in [
        "google_calendar",
        "google_drive",
        "google_docs",
        "google_sheets",
        "microsoft",
        "one",
        "onedrive",
    ] {
        let catalog = catalog_for_toolkit(alias).expect("alias resolves");
        assert!(distinct.contains(&catalog.as_ptr()), "{alias} is unlisted");
    }
}

#[test]
fn slack_channel_invite_is_admin() {
    // Adding someone to a channel exposes its history; the heuristic reads
    // INVITE as a plain read, so the curated scope must not be Write.
    let slack = catalog_for_toolkit("slack").expect("slack catalog");
    let invite = slack
        .iter()
        .find(|t| t.slug == "SLACK_INVITE_USERS_TO_A_SLACK_CHANNEL")
        .expect("invite is curated");
    assert_eq!(invite.scope, crate::scope::ToolScope::Admin);
}

#[test]
fn catalogs_match_the_recorded_fixtures() {
    // The fixtures were captured from the host tables before the move; they
    // pin every slug, scope and description in catalog order.
    let golden: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/catalog-actions.json")).expect("fixture");
    for (toolkit, expected) in &golden {
        let actual: Vec<serde_json::Value> = catalog_for_toolkit(toolkit)
            .map(|c| {
                c.iter()
                    .map(|t| serde_json::json!({"slug": t.slug, "scope": t.scope}))
                    .collect()
            })
            .unwrap_or_default();
        let expected = if expected.is_null() {
            Vec::new()
        } else {
            expected.as_array().expect("array").clone()
        };
        assert_eq!(actual, expected, "catalog changed for {toolkit}");
    }
    let descriptions: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("fixtures/descriptions.json")).expect("fixture");
    for (toolkit, expected) in &descriptions {
        assert_eq!(
            toolkit_description(toolkit),
            expected,
            "description of {toolkit}"
        );
    }
}

#[test]
fn catalog_for_toolkit_resolves_every_capability_toolkit() {
    // Every toolkit the capability surface reports on must have a catalog —
    // that is what `curated_tools: true` in the matrix claims about it.
    for toolkit in CAPABILITY_TOOLKITS {
        assert!(
            catalog_for_toolkit(toolkit).is_some(),
            "no curated catalog for advertised toolkit {toolkit}"
        );
    }
}

#[test]
fn catalog_for_toolkit_honours_slug_aliases() {
    // Multi-segment `ONE_DRIVE_*` extracts "one_drive"; "one" is a legacy alias.
    // Both UI/backend spellings remain accepted.
    for alias in ["one", "one_drive", "onedrive", "OneDrive"] {
        assert!(
            catalog_for_toolkit(alias).is_some(),
            "OneDrive alias {alias} did not resolve"
        );
    }
    // The legacy "microsoft" alias still reaches the Teams catalog.
    assert_eq!(
        catalog_for_toolkit("microsoft").map(<[CuratedTool]>::len),
        catalog_for_toolkit("microsoft_teams").map(<[CuratedTool]>::len)
    );
    for alias in ["google_calendar", "googlecalendar", "GOOGLECALENDAR"] {
        assert!(
            catalog_for_toolkit(alias).is_some(),
            "{alias} did not resolve"
        );
    }
    assert!(
        catalog_for_toolkit("  gmail  ").is_some(),
        "slug is not trimmed"
    );
    assert!(catalog_for_toolkit("nonexistent-toolkit").is_none());
}

#[test]
fn every_native_provider_has_a_catalog() {
    for slug in NATIVE_PROVIDERS {
        assert!(
            catalog_for_toolkit(slug).is_some(),
            "native provider {slug} has no curated catalog"
        );
        assert!(has_native_provider(slug));
        assert!(
            CAPABILITY_TOOLKITS.contains(slug),
            "native provider {slug} is missing from the capability list"
        );
    }
    assert!(!has_native_provider("jira"));
    assert!(!has_native_provider("nonexistent-toolkit"));
}

#[test]
fn toolkit_description_is_populated_for_every_capability_toolkit() {
    let generic = toolkit_description("definitely-not-a-real-toolkit");
    for toolkit in CAPABILITY_TOOLKITS {
        let d = toolkit_description(toolkit);
        assert!(!d.trim().is_empty(), "{toolkit} has an empty description");
        assert_ne!(
            d, generic,
            "{toolkit} falls through to the generic description"
        );
    }
}

#[test]
fn toolkit_description_recognizes_the_legacy_microsoft_alias() {
    // `catalog_for_toolkit` resolves both "microsoft" and "microsoft_teams" to
    // the same Teams catalog; the description must not diverge for the alias.
    assert_eq!(
        toolkit_description("microsoft"),
        toolkit_description("microsoft_teams")
    );
    assert_ne!(
        toolkit_description("microsoft"),
        toolkit_description("definitely-not-a-real-toolkit")
    );
}

#[test]
fn curated_catalogs_carry_no_duplicate_slugs() {
    // `find_curated` returns the first match, so a duplicate with a different
    // scope would make the gate's answer depend on table order.
    for toolkit in CAPABILITY_TOOLKITS {
        let catalog = catalog_for_toolkit(toolkit).expect("catalog");
        let mut seen: Vec<&str> = catalog.iter().map(|t| t.slug).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "{toolkit} catalog has duplicate slugs");
    }
}

#[test]
fn canonical_toolkit_folds_aliases_case_and_whitespace() {
    for (input, canonical) in [
        ("google_calendar", "googlecalendar"),
        ("  GoogleCalendar ", "googlecalendar"),
        ("google_drive", "googledrive"),
        ("google_docs", "googledocs"),
        ("google_sheets", "googlesheets"),
        ("microsoft", "microsoft_teams"),
        ("one", "one_drive"),
        ("onedrive", "one_drive"),
        ("OUTLOOK", "outlook"),
    ] {
        assert_eq!(canonical_toolkit(input), canonical, "{input}");
    }
    // A toolkit nobody aliases passes through, lowercased.
    assert_eq!(canonical_toolkit("Nonexistent-Toolkit"), "nonexistent-toolkit");
}
