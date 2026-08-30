//! Making a provider's text worth ingesting.
//!
//! What a provider returns is not what should go into a user's memory. A single
//! marketing email carries the message, then the quoted thread it replied to,
//! then an unsubscribe block, a legal disclaimer, and a postal address. Ingested
//! whole, the noise outweighs the content — and it is *the same* noise in every
//! message, so it dominates any similarity search run over the result.
//!
//! # These are conservative on purpose
//!
//! Every rule here can only cut, never rewrite, and each trigger is text that
//! could not reasonably appear inside real prose. The failure modes are not
//! symmetrical: leaving a footer in costs some tokens and a little search
//! noise, while cutting into a message loses something the user wrote and will
//! never know is missing.

mod email;

pub use email::{clean_body, collapse_blank_runs, drop_footer_noise, drop_reply_chain, truncate};

#[cfg(test)]
mod test;
