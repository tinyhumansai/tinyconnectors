//! Who wrote an item, as an address and a display name.
//!
//! Providers render a sender as `Name <address>`, `"Name" <address>` or a bare
//! address. Only an email address is kept: a sender named by a phone number
//! (an SMS gateway, a messaging bridge) is left out, and so is a display name
//! holding one, because a phone number is never stored.

use serde_json::Value;
use tinyconnectors_bus::RecordSender;

use super::json::pick_str;

/// The sender of `item`: the first of `paths` holding one, else the `From`
/// header of a Gmail-shaped `payload.headers`. `None` when `paths` is empty.
pub(super) fn sender_of(item: &Value, paths: &[&str]) -> Option<RecordSender> {
    if paths.is_empty() {
        return None;
    }
    let raw = pick_str(item, paths).or_else(|| from_header(item))?;
    parse(&raw)
}

/// The `From` header among `payload.headers`.
fn from_header(item: &Value) -> Option<String> {
    item.pointer("/payload/headers")?
        .as_array()?
        .iter()
        .find(|header| {
            header["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case("from"))
        })
        .and_then(|header| header["value"].as_str())
        .map(str::to_owned)
}

/// `Name <address>` or a bare address as a sender; `None` for anything that
/// is not an email address, or is a phone number's.
pub(super) fn parse(raw: &str) -> Option<RecordSender> {
    let raw = raw.trim();
    let (name, address) = if let Some((name, rest)) = raw.rsplit_once('<') {
        (
            name.trim().trim_matches('"').trim(),
            rest.strip_suffix('>')?.trim(),
        )
    } else {
        ("", raw)
    };
    let (local, domain) = address.split_once('@')?;
    let well_formed = !local.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && !address
            .chars()
            .any(|c| c.is_whitespace() || c == '<' || c == '>');
    if !well_formed || looks_like_phone(local) {
        return None;
    }
    let name = Some(name)
        .filter(|name| !name.is_empty() && *name != address && !holds_phone(name))
        .map(str::to_owned);
    Some(RecordSender {
        address: address.to_owned(),
        name,
    })
}

/// Only phone characters, with enough digits to dial.
fn looks_like_phone(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_digit() || "+-(). ".contains(c))
        && holds_phone(text)
}

/// Seven or more digits: enough to be a phone number.
fn holds_phone(text: &str) -> bool {
    text.chars().filter(char::is_ascii_digit).count() >= 7
}

#[cfg(test)]
#[path = "sender_tests.rs"]
mod test;
