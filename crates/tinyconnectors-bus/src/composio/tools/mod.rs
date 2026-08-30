//! Function-calling schemas for a toolkit's actions.
//!
//! These are kept in the `OpenAI` `{ "type": "function", "function": {…} }`
//! envelope the backend already wraps Composio's shape in, so a caller can
//! forward one straight into a model request without a translation step.

mod types;

pub use types::{
    ComposioListToolsRequest, ComposioToolFunction, ComposioToolSchema, ComposioToolsResponse,
};

#[cfg(test)]
mod test;
