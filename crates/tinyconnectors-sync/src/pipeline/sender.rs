//! Who wrote an item, as an address and a display name.
//!
//! Providers render a sender as `Name <address>`, `"Name" <address>` or a bare
//! address. The address is an email address (an SMS or MMS gateway's
//! `+15551234567@sms.example.com` included) or a phone number, kept as its
//! dial digits (`+15551234567`) so it is one token wherever it is used.

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
/// is neither an email address nor a phone number.
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
    let address = if looks_like_phone(address) {
        dial_digits(address)
    } else if is_email(address) {
        address.to_owned()
    } else {
        return None;
    };
    let name = Some(name)
        .filter(|name| !name.is_empty() && *name != address)
        .map(str::to_owned);
    Some(RecordSender { address, name })
}

/// One `@`, a non-empty local part, a dotted domain, no whitespace.
fn is_email(address: &str) -> bool {
    address.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && domain.contains('.') && !domain.contains('@')
    }) && !address
        .chars()
        .any(|c| c.is_whitespace() || c == '<' || c == '>')
}

/// Only phone characters, with enough digits (seven) to dial.
fn looks_like_phone(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_digit() || "+-(). ".contains(c))
        && text.chars().filter(char::is_ascii_digit).count() >= 7
}

/// `text`'s digits, with its leading `+` when it has one.
fn dial_digits(text: &str) -> String {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if text.starts_with('+') {
        format!("+{digits}")
    } else {
        digits
    }
}

#[cfg(test)]
#[path = "sender_tests.rs"]
mod test;
