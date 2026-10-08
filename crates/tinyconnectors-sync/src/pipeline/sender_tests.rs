//! Unit tests for reading an item's sender.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{parse, sender_of};
use tinyconnectors_bus::RecordSender;

fn sender(address: &str, name: Option<&str>) -> RecordSender {
    RecordSender {
        address: address.into(),
        name: name.map(str::to_owned),
    }
}

#[test]
fn reads_a_named_a_quoted_and_a_bare_address() {
    assert_eq!(
        parse("Priya Shah <priya@acme.com>"),
        Some(sender("priya@acme.com", Some("Priya Shah")))
    );
    assert_eq!(
        parse("\"Shah, Priya\" <priya@acme.com>"),
        Some(sender("priya@acme.com", Some("Shah, Priya")))
    );
    assert_eq!(
        parse(" priya@acme.com "),
        Some(sender("priya@acme.com", None))
    );
    assert_eq!(
        parse("priya@acme.com <priya@acme.com>"),
        Some(sender("priya@acme.com", None)),
        "a name that is only the address adds nothing"
    );
}

#[test]
fn never_keeps_a_phone_number() {
    assert_eq!(parse("+15551234567@sms.example.com"), None);
    assert_eq!(parse("Text <(555) 123-4567@mms.example.com>"), None);
    assert_eq!(parse("+1 555 123 4567"), None, "not an address at all");
    assert_eq!(
        parse("Call me 555 123 4567 <priya@acme.com>"),
        Some(sender("priya@acme.com", None)),
        "the address stays, the name holding a number goes"
    );
}

#[test]
fn refuses_what_is_not_an_address() {
    for raw in [
        "",
        "Priya",
        "priya@localhost",
        "Priya <priya@acme.com",
        "a@b@c.com",
    ] {
        assert_eq!(parse(raw), None, "{raw}");
    }
}

#[test]
fn falls_back_to_the_from_header_and_reads_nothing_without_paths() {
    let item = json!({
        "payload": { "headers": [
            { "name": "Subject", "value": "Lunch?" },
            { "name": "From", "value": "Priya <priya@acme.com>" }
        ] }
    });
    assert_eq!(
        sender_of(&item, &["sender"]),
        Some(sender("priya@acme.com", Some("Priya")))
    );
    assert_eq!(sender_of(&item, &[]), None);

    let direct = json!({ "sender": "Sam <sam@acme.com>", "payload": item["payload"] });
    assert_eq!(
        sender_of(&direct, &["sender"]),
        Some(sender("sam@acme.com", Some("Sam"))),
        "a path wins over the header"
    );
}
