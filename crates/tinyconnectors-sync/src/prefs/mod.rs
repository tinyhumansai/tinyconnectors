//! What the user has allowed an agent to do with each toolkit.
//!
//! A connected account can do more than most users want an agent doing with it
//! unsupervised. The preference is per toolkit and per scope: read a mailbox,
//! yes; send from it, perhaps; delete from it, probably not.
//!
//! # The default allows reads and writes, not admin
//!
//! Read alone makes most integrations useless — an agent that can see a
//! calendar but not add to it is a worse version of looking yourself. Admin is
//! off because its actions are the ones that destroy things, and a user who
//! wants that should have to say so.
//!
//! # A missing preference reads as the default, deliberately
//!
//! There is no "unset" state a caller has to handle, and no failure path where
//! an unreadable preference silently becomes "allow everything". A store that
//! cannot be read is an error; a store with nothing in it is a user who has not
//! chosen yet.

mod types;

pub use types::{PREFS_NAMESPACE, UserScopePref};

#[cfg(test)]
mod test;
