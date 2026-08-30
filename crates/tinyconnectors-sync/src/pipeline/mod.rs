//! Driving one sync run.
//!
//! A provider knows how to read one page of its toolkit. Everything around that
//! page is the same for every toolkit, and lives here:
//!
//! - Loading the connection's cursor so a run resumes rather than restarting.
//! - Refusing to start when the day's request budget is spent.
//! - Paging until the item limit, the budget, or the provider runs out.
//! - Dropping items already ingested, and re-ingesting ones that changed.
//! - Saving the cursor so the next run picks up where this one stopped.
//!
//! # Why the provider does not loop
//!
//! [`crate::ConnectorProvider::fetch_page`] reads exactly one page. The loop is here
//! because the things that stop it — a budget, an item limit, a duplicate — are
//! not the provider's business, and a provider that looped internally would
//! have to re-implement all three, differently, five times.
//!
//! # Why a run that fails part-way still saves its cursor
//!
//! A sync that read four pages and failed on the fifth has genuinely made
//! progress, and the records it did read are returned. Throwing the cursor away
//! would mean re-reading those four pages on every attempt — which, for a
//! connection failing consistently, is the difference between a wasted page and
//! a wasted mailbox.

mod fetch;
mod json;
mod page_size;
mod run;

pub use fetch::{PageSpec, fetch_page};
pub use json::{first_array, next_page_token, pick_str};
pub use page_size::{MIN_PAGE_SIZE, is_payload_too_large, shrink_page_size};
pub use run::{ProviderPage, SyncOutcome, run_sync};

#[cfg(test)]
mod test;
