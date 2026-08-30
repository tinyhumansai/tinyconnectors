//! The per-day provider request allowance.

use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Requests a connection may make in one day before the budget stops it.
pub const DEFAULT_DAILY_REQUEST_LIMIT: u32 = 500;

/// Today's civil date, UTC, as `YYYY-MM-DD`.
///
/// UTC rather than local: the budget is persisted and compared across runs, and
/// a machine that changes timezone must not appear to gain or lose a day.
fn today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

/// How much of a day's provider request allowance is spent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyBudget {
    /// Civil date the count applies to, `YYYY-MM-DD` in UTC.
    pub date: String,
    /// Requests made on [`Self::date`].
    pub requests_used: u32,
    /// Requests permitted per day.
    pub limit: u32,
}

impl Default for DailyBudget {
    fn default() -> Self {
        Self {
            date: today(),
            requests_used: 0,
            limit: DEFAULT_DAILY_REQUEST_LIMIT,
        }
    }
}

impl DailyBudget {
    /// Requests still available today.
    ///
    /// A stored budget from an earlier date reports the full limit: the
    /// rollover happens on read, so a budget that was never written back does
    /// not keep suppressing runs into the next day.
    #[must_use]
    pub fn remaining(&self) -> u32 {
        if self.date == today() {
            self.limit.saturating_sub(self.requests_used)
        } else {
            self.limit
        }
    }

    /// Whether today's allowance is spent.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.remaining() == 0
    }

    /// Charge `count` requests, rolling over first if the date has changed.
    ///
    /// Saturating: a run that somehow makes more calls than the limit records
    /// an exhausted budget rather than wrapping to nearly-unused, which would
    /// hand it a fresh allowance at the worst possible moment.
    pub fn record_requests(&mut self, count: u32) {
        let today = today();
        if self.date != today {
            self.date = today;
            self.requests_used = 0;
        }
        self.requests_used = self.requests_used.saturating_add(count);
    }
}
