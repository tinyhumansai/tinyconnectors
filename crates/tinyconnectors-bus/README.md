# tinyconnectors-bus

Every type that crosses the TinyConnectors module's `TinyBus` boundary, and the
names of the members that carry them.

TinyConnectors ships as a loadable module so a host does not compile the
implementation: `crates/tinyconnectors` is built as a `cdylib` and exports one
object. A host can load that binary but cannot `use` anything out of it, so the
payload vocabulary has to be published as an ordinary library. This is it.

| module         | what it holds                                                 |
| -------------- | ------------------------------------------------------------- |
| `names`        | interface name, object path, one constant per member          |
| `composio`     | the Composio backend's value vocabulary, by payload family    |
| `version`      | `CONTRACT_VERSION` and the bind rule a host applies to it     |
| storage needs  | canonical `StorageNeeds` and collection schema from TSD       |

`composio` holds six families — `toolkits`, `connections`, `tools`, `execute`,
`triggers`, `github` — all re-exported at the crate root. Composio is one OAuth
connector backend, not the only one this contract expects to carry, so it is
namespaced: a second backend arrives as a sibling module with its own interface
and object path rather than as a rename of every type here.

Three data dependencies: `serde`, `serde_json`, and the canonical
`tinystoragedrivers-bus` crate. That dependency is pinned by the workspace to
the released storage contract with default features disabled, so its optional
TinyBus client is not linked. The optional `webhook` feature adds `hmac`,
`sha2`, `base64` and `thiserror` — still pure Rust, still no I/O.

## This crate sits underneath `tinyconnectors`

`tinyconnectors` **depends on this crate and re-exports all of it**. That direction
matters, and it is the opposite of the obvious one.

A *host* needs the payload types and needs nothing else: it loads the module and
makes calls, so it names `ComposioConnection` and `ComposioAuthorizeResponse` but
implements no behavior and links no transport. Making it depend on the whole module crate — and
through it on `tinybus`, `tokio`, and the module SDK — to spell a payload type
would be the wrong shape.

The alternative, a parallel set of payload types for hosts, is worse: a
`ComposioConnection` defined twice is two distinct types, with a conversion at
every call site that nothing checks. One definition, here, at the bottom. That
is not hypothetical — these types already lived in `tinymemory-api` because two
crates needed to name them and there was nowhere else both could.

Because the re-export is by module as well as by item,
`tinyconnectors::ComposioConnection`, `tinyconnectors::names::OBJECT_PATH`, and
`tinyconnectors_bus::composio::connections::ComposioConnection` all resolve to
the same items, not twins.

So: a module author depends on `tinyconnectors` and gets behavior and vocabulary. A
host depends on `tinyconnectors-bus` and gets vocabulary alone.

## What is deliberately absent

**No behavior.** The client, the OAuth handoff, and the transport live in
`crates/tinyconnectors`. A payload type describes what a frame carries, not what
the module does with it. The split is readable off the path: a name here is
data, a name there is an obligation.

One opt-in exception, behind the `webhook` feature:
`composio::triggers::webhook` verifies a Composio webhook delivery's signature
(`verify`) and decodes its body into a `ComposioTriggerEvent` (`decode`), for a
host that receives deliveries at its own public endpoint. It lives here because
that host links this crate, not the module. Both functions are pure: bytes and
a caller-supplied `now` in, a verdict out, no transport and no clock.

**No credentials.** Nothing here holds an API key, a token, or a refresh secret.
The OAuth handoff crosses this boundary as a URL the user opens and an id to
poll — never as a token — so a host that links this crate has nothing worth
leaking.

**No transport.** This crate does not depend on `tinybus` and holds no
connection, client, or codec. A host already owns its connection — its reconnect
policy, its timeouts, its tracing — and the useful part is the vocabulary.

That is also structural, not just preference: `tinybus` is vendored as a
submodule whose manifest inherits fields from its own nested
`[workspace.package]`. The TSD dependency's default client feature is disabled,
keeping this contract transport-free. CI checks the normal/build dependency
graph from an independent consumer workspace.

## Storage needs

`STORAGE_NEEDS` is the concrete declaration exported by this crate; its
`StorageNeeds` value and `CollectionSpec`, `IndexSpec` and `SearchSpec` types
come directly from `tinystoragedrivers-bus`. It describes requested resources;
the host remains responsible for choosing and binding the grant. TinyConnectors
requests the `composio-user-scopes` collection with no indexes, TTL field or
search schema, plus the `triggers/` stream prefix. It requests no blob or secret
prefixes.

## Making a call

Arguments travel as a positional JSON array — `#[tinybus::interface]` decodes
them into a tuple — and the member name comes from `names`:

```rust,ignore
use tinyconnectors_bus::{names, ComposioAuthorizeRequest, ComposioAuthorizeResponse};

let proxy = connection.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
let reply: ComposioAuthorizeResponse = proxy
    .call(
        names::methods::AUTHORIZE,
        (ComposioAuthorizeRequest { toolkit: "gmail".into(), extra_params: None },),
    )
    .await?;
// The handoff is not finished here: the user opens this URL in a browser, and
// the connection it names stays inactive until they do.
println!("{}", reply.connect_url);
```

Nothing above is a string literal at a call site. Renaming the interface, the
path, or a member is therefore a compile error in every consumer rather than an
`UnknownMethod` discovered at runtime.

## Staying in step with the module

`names::METHODS` lists every member in dispatch order. `crates/tinyconnectors`
asserts its served members against that list, so a method added to the interface
without an entry here fails that crate's tests rather than surfacing in a host.

The table describes what the module serves, never what is planned. A constant
for a member nothing answers is discovered by a host as a runtime "unknown
method", which is strictly worse than the member not existing — so the remaining
Composio operations arrive as additive minor bumps rather than sitting here
unanswered.

## Versioning

Contract 1.12 removed the `Sync` member, the `records` module
(`ConnectorRecord`, `ConnectorRecordBatch`, `RecordSender`, `SyncEvent`,
`SyncStage`, `ConnectorSyncRequest`, `ConnectorSyncResponse`) and the sync
fields of `ComposioCapability` (`initial_sync`, `periodic_sync`,
`sync_interval_secs`, `memory_ingest`). The module no longer reads records out
of connected accounts; a host that wants account data runs actions through
`Execute`.

Contract 1.22 adds three optional fields: `webhooks` on the direct
`ComposioConfigureRequest` and on `ComposioDirectCredential`, which let a direct
route serve the trigger members for a host that receives deliveries itself, and
`user_id`, `connected_account_id` and `trigger_id` on `ComposioTriggerMetadata`.
Each is omitted from the wire when unset, so older peers see the same blobs.

`CONTRACT_VERSION` describes *this vocabulary*, not the package. Bump its major
component when a payload's wire form changes incompatibly or a member is removed
or renamed, and its minor component when a member or an optional field is added.
It is deliberately independent of the package version the release workflow owns,
which tracks the shipped artifact.

The payload tests pin the serde representation, because that representation is
the wire form: a host and a module that disagree about a field name fail at
runtime with a decode error, so the shape is asserted rather than assumed.

## Adding a payload family

One directory per family, with `mod.rs` explaining what the family is for,
`types.rs` holding the definitions, and `test.rs` pinning the serde form. Put a
Composio envelope under `composio/`; put something genuinely backend-neutral at
the root. Keep the crate dependency-light: the moment it links a transport or a
runtime, the reason it exists is gone.

Contract 1.13 adds processing DTOs and seven members for argument preparation,
task filtering, structured provider classification and leased trigger archives. These
are serialized vocabulary only; normalization, filtering and file I/O remain
in the compiled module. Existing member arities and wire forms are unchanged.
