//! Serde representation tests for the GitHub repository payloads.

use super::ComposioGithubReposResponse;
use serde_json::json;

#[test]
fn parses_camel_case_repository_fields() {
    let raw = json!({
        "connectionId": "c-1",
        "repositories": [{
            "owner": "tinyhumansai",
            "repo": "tinyconnectors",
            "fullName": "tinyhumansai/tinyconnectors",
            "private": true,
            "defaultBranch": "main",
            "htmlUrl": "https://github.com/tinyhumansai/tinyconnectors"
        }]
    });
    let resp: ComposioGithubReposResponse = serde_json::from_value(raw).expect("parses");
    assert_eq!(resp.connection_id, "c-1");
    let repo = &resp.repositories[0];
    assert_eq!(repo.full_name, "tinyhumansai/tinyconnectors");
    assert_eq!(repo.private, Some(true));
    assert_eq!(repo.default_branch.as_deref(), Some("main"));

    let value = serde_json::to_value(&resp).expect("serializes");
    assert!(value.get("connectionId").is_some());
    assert!(value["repositories"][0].get("fullName").is_some());
    assert!(value["repositories"][0].get("defaultBranch").is_some());
}

#[test]
fn defaults_an_absent_repository_list_to_empty() {
    let resp: ComposioGithubReposResponse =
        serde_json::from_value(json!({ "connectionId": "c-1" })).expect("parses");
    assert!(resp.repositories.is_empty());
}
