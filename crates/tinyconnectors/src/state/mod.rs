//! Where the module keeps the user's scope preferences.
//!
//! [`tinyconnectors_sync`] defines the preference store as a two-method
//! key-value seam ([`tinyconnectors_sync::PrefsStore`]) and deliberately does
//! not implement one. This module supplies the module's own.
//!
//! # Why a file per key
//!
//! A preference is read on every `Execute` and `ListTools` that applies user
//! scopes and written when the user changes it. A file per key means a corrupt
//! write damages one toolkit's preference rather than all of them, and the
//! whole thing is inspectable. The layout is
//! `<state_dir>/composio-user-scopes/<toolkit>.json` — unchanged from the
//! layout the same rows had when the store also held sync cursors, so existing
//! preferences keep reading.

mod file_store;

pub use file_store::FilePrefsStore;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
