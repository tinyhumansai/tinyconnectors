//! What a sync run has to remember between runs.
//!
//! A pipeline is otherwise stateless — it reads a provider and returns records.
//! Three things it cannot recompute:
//!
//! - **Where it got to.** A cursor, so the next run resumes instead of
//!   re-reading a user's entire mailbox.
//! - **What it has already seen.** Item ids and versions, so an item that has
//!   not changed is not re-ingested as new.
//! - **How much of today's budget is left.** Provider calls cost money and
//!   count against rate limits, so a run that goes wrong must not be able to
//!   spend the day's allowance in a loop.
//!
//! # The store is the host's
//!
//! [`SyncStateStore`] is two methods over JSON. That is deliberately as small
//! as it is: this crate must not depend on memory, and a seam any wider would
//! start describing a storage engine. A host backs it with whatever it already
//! has.
//!
//! # Why the budget rolls over by calendar date
//!
//! [`DailyBudget`] resets when the civil date changes, not after a fixed number
//! of elapsed seconds. A user's "500 requests a day" is a day in their calendar
//! — a sliding twenty-four-hour window would leave a heavy morning suppressing
//! the following morning, which reads as the integration having broken.

mod budget;
mod store;

pub use budget::{DEFAULT_DAILY_REQUEST_LIMIT, DailyBudget};
pub use store::{STATE_NAMESPACE, SyncState, SyncStateStore};

#[cfg(test)]
mod test;
