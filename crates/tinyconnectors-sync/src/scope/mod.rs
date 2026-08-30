//! How invasive an action is, and which actions a provider surfaces at all.
//!
//! Composio publishes sixty-odd actions for a typical toolkit, and most are
//! noise for an agent's planning loop. Two mechanisms cut that down:
//!
//! - A provider's **curated catalog** — a hand-picked list of the actions worth
//!   offering, each tagged with a [`ToolScope`].
//! - A user's **scope preference** — read, write, or admin — checked against
//!   that tag before an action is offered or run.
//!
//! # Why an uncurated action still gets a scope
//!
//! Not every toolkit has a curated catalog, and a user's scope preference has
//! to mean something for those too. [`classify_unknown`] derives a scope from
//! the action's verb. It is a heuristic and it is deliberately cautious: the
//! destructive verbs are checked first, so `MODIFY_LABELS` is admin rather than
//! slipping into write on its `MODIFY` substring.
//!
//! Getting this wrong in the safe direction costs a user an action they wanted.
//! Getting it wrong in the other direction deletes their mail.

mod types;

pub use types::{CuratedTool, ToolScope, classify_unknown, find_curated, toolkit_from_slug};

#[cfg(test)]
mod test;
