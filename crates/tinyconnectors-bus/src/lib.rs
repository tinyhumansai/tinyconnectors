//! Every type that crosses the `TinyConnectors` module's `TinyBus` boundary, and
//! the names of the members that carry them.
//!
//! `TinyConnectors` owns the OAuth-connector surface: linking a user's accounts,
//! listing what those accounts can do, running actions against them, and
//! subscribing to the webhooks they emit. `crates/tinyconnectors` ships that as
//! a loadable `TinyBus` module — built as a `cdylib`, exporting one object. A
//! host that loads that binary can call into it but cannot `use` anything out
//! of it, so the payload vocabulary has to be published as an ordinary library.
//! This is that library.
//!
//! # What is here
//!
//! - [`names`] — the interface name, the object path, and one constant per
//!   member, plus [`names::METHODS`] listing them in dispatch order.
//! - [`composio`] — the value vocabulary of the Composio backend, one module
//!   per payload family. Its types are re-exported at this root.
//! - [`version`] — [`CONTRACT_VERSION`] and the [`is_compatible`] bind rule.
//!
//! # One backend today, room for more
//!
//! Composio is the connector backend this contract carries, not the only one it
//! expects to. Everything Composio-shaped is namespaced under [`composio`] so a
//! second backend arrives as a sibling module with its own interface and object
//! path, rather than as a rename of every type here. The types keep their
//! `Composio` prefix deliberately: they mirror Composio's own response
//! envelopes, and dressing them as a neutral abstraction would be a lie the
//! first time a second backend disagreed about a field.
//!
//! # What is deliberately not here
//!
//! **No behavior.** The client, the OAuth handoff, the sync pipelines, and the
//! agent tools all live in `crates/tinyconnectors`, which depends on this crate
//! and re-exports it. A payload type describes what a frame carries, not what
//! the module does with it.
//!
//! **No transport.** This crate does not depend on `tinybus` and holds no
//! connection, client, or codec. A host already owns its connection — its
//! reconnect policy, its timeouts, its tracing — and the useful part is the
//! vocabulary, not another wrapper around it.
//!
//! That is also a structural necessity, not only a preference: `tinybus` is
//! vendored as a submodule whose manifest inherits fields from its own nested
//! `[workspace.package]`. A crate that every workspace member can depend on has
//! to stay transport-free, and staying transport-free is what keeps this crate
//! down to two pure-Rust dependencies.
//!
//! **No credentials.** Nothing here holds an API key, a token, or a refresh
//! secret. The OAuth handoff crosses this boundary as a URL the user opens and
//! an id to poll — never as a token — so a host that links this crate has
//! nothing worth leaking.
//!
//! # This crate sits underneath the implementation, not beside it
//!
//! `tinyconnectors` **depends on this crate and re-exports all of it**, so
//! `tinyconnectors::ComposioConnection` and
//! `tinyconnectors_bus::composio::ComposioConnection` are the *same type*, not
//! structural twins. Defining a parallel set of payload types for hosts would
//! mean a conversion at every call site that nothing checks. One definition,
//! here, at the bottom.
//!
//! So: a module author depends on `tinyconnectors` and gets behavior and
//! vocabulary. A host depends on `tinyconnectors-bus` and gets vocabulary
//! alone.
//!
//! # Staying in step with the module
//!
//! [`names::METHODS`] lists every member. `crates/tinyconnectors` asserts its
//! served members against that list, in order, so a method added to the
//! interface without an entry here fails that crate's tests rather than
//! surfacing as an unknown method in a host at runtime.
//!
//! # Example
//!
//! ```
//! use tinyconnectors_bus::{names, ComposioAuthorizeResponse, ComposioConnection};
//!
//! assert_eq!(names::methods::AUTHORIZE, "Authorize");
//! assert_eq!(names::OBJECT_PATH, "/ai/tinyhumans/connectors/Composio");
//!
//! // The OAuth handoff: a hosted URL to open, and the row it will activate.
//! let reply: ComposioAuthorizeResponse = serde_json::from_value(serde_json::json!({
//!     "connectUrl": "https://composio.dev/oauth/abc",
//!     "connectionId": "conn_1",
//! }))?;
//! assert_eq!(reply.connection_id, "conn_1");
//!
//! // Until the user finishes it, the row is not usable.
//! let pending: ComposioConnection = serde_json::from_value(serde_json::json!({
//!     "id": "conn_1", "toolkit": "gmail", "status": "PENDING",
//! }))?;
//! assert!(!pending.is_active());
//! # Ok::<(), serde_json::Error>(())
//! ```

pub mod composio;
pub mod names;
pub mod version;

pub use composio::{
    ComposioActiveTrigger, ComposioActiveTriggersResponse, ComposioAgentReadyToolkitsResponse,
    ComposioAuthorizeResponse, ComposioAvailableTrigger, ComposioAvailableTriggerRepo,
    ComposioAvailableTriggersResponse, ComposioCapabilitiesResponse, ComposioCapability,
    ComposioConnection, ComposioConnectionsResponse, ComposioCreateTriggerResponse,
    ComposioDeleteResponse, ComposioDisableTriggerResponse, ComposioEnableTriggerResponse,
    ComposioExecuteResponse, ComposioGithubRepo, ComposioGithubReposResponse,
    ComposioToolFunction, ComposioToolSchema, ComposioToolkitCatalogEntry,
    ComposioToolkitsResponse, ComposioToolsResponse, ComposioTriggerEvent,
    ComposioTriggerHistoryEntry, ComposioTriggerHistoryResult, ComposioTriggerMetadata,
};
pub use names::{INTERFACE, METHODS, OBJECT_PATH};
pub use version::{CONTRACT_VERSION, is_compatible};
