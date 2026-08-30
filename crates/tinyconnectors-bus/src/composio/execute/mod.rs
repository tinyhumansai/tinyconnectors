//! The result of running one action against a connected account.
//!
//! Execution is metered: [`ComposioExecuteResponse::cost_usd`] carries what the
//! call cost the caller, which is why the response is a typed envelope rather
//! than the provider's raw JSON.

mod types;

pub use types::ComposioExecuteResponse;

#[cfg(test)]
mod test;
