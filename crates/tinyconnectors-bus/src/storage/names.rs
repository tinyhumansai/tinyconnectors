//! Storage names and versions. Durable: renaming one strands stored data.

/// The collection holding one document per toolkit (schema v2).
///
/// Durable: changing it strands every user's saved choices, and each toolkit
/// silently reads as the default again. Pinned by a test. A rename needs a
/// migration step from the old name, as [`LEGACY_PREFS_NAMESPACE`] has one.
pub const PREFS_NAMESPACE: &str = "connectors.user_scopes";

/// The v1 name of [`PREFS_NAMESPACE`], whose documents wrapped the preference
/// as `{ "value": … }`. Only the migration step reads it.
pub const LEGACY_PREFS_NAMESPACE: &str = "composio-user-scopes";

/// The body version [`PREFS_NAMESPACE`] documents are written at.
pub const PREFS_SCHEMA_VERSION: u32 = 2;

/// Reserved field holding a preference that is not a plain object.
pub const NON_OBJECT_FIELD: &str = "non_object_value";

/// The stream family the trigger archive writes, one stream per UTC day.
pub const STREAM_NAME: &str = "connectors.triggers";

/// Prefix of every day's stream: [`STREAM_NAME`] and a slash.
pub const STREAM_PREFIX: &str = "connectors.triggers/";

/// The v1 prefix of a day's stream, renamed to [`STREAM_PREFIX`] by migration.
pub const LEGACY_STREAM_PREFIX: &str = "triggers/";
