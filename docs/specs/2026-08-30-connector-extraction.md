# Connector extraction

**Status:** accepted. Phase 1 landed; phases 2–5 planned in
[`docs/plans/2026-08-30-connector-extraction.md`](../plans/2026-08-30-connector-extraction.md).

## What this is

OAuth connector integrations currently live in two repositories that each own
part of one feature. This workspace takes both halves and serves them over
TinyBus, so `openhuman` and `tinycortex` load connectors instead of compiling
them.

The two halves today:

| Where | What | Size |
| --- | --- | --- |
| `openhuman/src/openhuman/integrations/composio/` | client, OAuth handoff, RPC controllers, agent tools, catalog, triggers, contract gate | ~21k lines |
| `tinymemory/crates/tinymemory-core/src/sync/composio/` | provider registry, per-provider catalogs, profile and scope handling | ~12k lines |
| `tinymemory/crates/tinymemory-core/src/sync/pipelines/composio/` | per-provider sync pipelines and their orchestrator | ~6.5k lines |
| `tinymemory/crates/tinymemory-sync/` | provider-specific record post-processing | ~3.3k lines |

The payload types already moved once, from OpenHuman into
`tinymemory-api::host::composio`, because both halves read them. That move is
the evidence for this one: the types ended up in the memory contract crate not
because they belong to memory, but because there was nowhere else both crates
could name. This workspace is that place.

## Naming

The module is `tinyconnectors`, not `tinycomposio`. Composio is one OAuth
connector backend and the design assumes it will not be the only one.

The rule applied throughout:

- **Neutral** — the crates, the `connectors` path segment, and anything that is
  not backend-specific: the OAuth handoff policy, the transport seam, the error
  type.
- **`composio`** — the module namespace holding one backend's vocabulary, its
  client, its paths, and its interface. A second backend arrives as a sibling
  with its own interface and object path, not as a rename of the first.
- **`Composio`-prefixed types** — kept. These mirror Composio's own response
  envelopes. Renaming them to look neutral would misrepresent them the first
  time a second backend disagreed about a field, and would churn every call
  site in both source repositories for nothing.

## Constraints

### The module holds no connector credential

The module never calls Composio. Every request goes through a backend that owns
the Composio API key, the billing margin, the toolkit allowlist, and the HMAC
verification of inbound webhooks.

That backend authenticates the *user*, and the user's credential belongs to the
host. So the module takes a `base_url` and an `auth_token` in its configuration
blob at load time, holds them in the transport, and never logs, returns, or
interpolates them. `HttpTransport`'s `Debug` is hand-written to omit the token.

### The contract crate stays dependency-light and transport-free

`tinyconnectors-bus` depends on `serde` and `serde_json` and nothing else. A
host that only makes calls links it alone. This is also structural: `tinybus` is
vendored as a submodule whose manifest inherits from its own nested
`[workspace.package]`, so a crate every member can depend on has to stay
transport-free.

### Routing is the module's job; choosing a route is the host's

Composio is reachable two ways and the module implements both:

- **proxy** — through the TinyHumans backend at `/agent-integrations/composio/*`,
  which owns the Composio key, the margin, the allowlist, and webhook
  verification.
- **direct** — at `backend.composio.dev/api/v3` with a user-supplied
  `x-api-key`.

They differ in base URL, auth header, paths, *and response shape*, so a route is
not a transport: it owns its paths and the translation of its responses into
this contract's envelopes. `ComposioClient` holds only what is true either way —
argument validation, the reserved-key refusal, the required-scope merge — so
nothing above it branches on mode.

Selection stays host policy. Whether the user is signed in, whether they
supplied a key, and which the product prefers are all decisions upstream of this
crate; the host states its choice in the module configuration blob.

**The routes are not equivalent, and the module says so.** Direct mode has no
per-user toolkit allowlist — you see Composio's whole catalog — so `ListToolkits`
has nothing to return, and the proxy's `DeleteConnection` also clears memory
sourced from the connection, which Composio knows nothing about. Both answer
with `Error::UnsupportedByRoute` naming the route and the member. An empty list
would read as "you may connect nothing"; a bare v3 delete would silently leave
the user's synced content behind.

### A direct-mode key that keeps failing gets gated

A revoked key fails identically forever, and the connection list is polled every
few seconds. `DirectRoute` counts consecutive authentication failures per key
fingerprint — never per key — and stops calling after three. A single success
clears the count; failures that say nothing about the key (a 502, a dropped
connection) do not count toward it, so an outage cannot gate a working key.

### A credential never leaves over a URL that could redirect it

`HttpTransport` validates its base URL before any request: HTTPS, or a genuine
loopback address for local development. The check parses rather than
prefix-matches, because `starts_with("http://127.0.0.1")` is fooled by userinfo
smuggling — `http://127.0.0.1:8080@evil.com` has host `evil.com`, and an HTTP
client routes it there carrying the credential header. Embedded credentials in
the URL are refused outright.

### Sync returns records; it does not write memory

The pipelines currently reach memory's store directly, which is why they cannot
move without taking half of memory with them. After the migration a sync
*returns* `ConnectorRecordBatch`, the host hands it to memory over memory's own
bus API, and neither side links the other.

`ConnectorRecord`'s fields and wire names are memory's ingestion vocabulary
exactly — asserted in `records/test.rs` against a literal key list, not imported
from the memory contract, since importing it would reintroduce the coupling this
crate exists to remove. Provenance memory does not ingest (toolkit, connection)
rides on the batch, because it describes the run rather than any record.

### Wire shapes are not ours to choose

Every payload mirrors a response envelope from the OpenHuman backend, which
forwards Composio's shapes. Field names and `#[serde]` attributes are a
contract: a host and a module that disagree about a field name fail at runtime
with a decode error. Each payload family pins its representation in a `test.rs`.

Composio has repeatedly turned a stringy field into an object carrying the
string plus render metadata. `ComposioActiveTrigger` decodes its required
fields through deserializers that accept both, because a strict `String` field
empties a user's trigger list — which reads as their subscriptions having been
silently deleted.

### The member table describes what is served, not what is planned

`names::METHODS` lists only members the module answers. A constant for a member
nothing serves is discovered by a host as a runtime "unknown method", which is
strictly worse than the member not existing. Members arrive as additive minor
bumps of `CONTRACT_VERSION`.

## The seams

Three seams carry this migration, and each replaces something host-specific:

| Seam | Replaces | Why it is a seam |
| --- | --- | --- |
| `client::Transport` | `openhuman::integrations::IntegrationClient` | The credential and its acquisition are the host's. Baking one host's Bearer-JWT flow in would make this that host's library. |
| `client::Route` | `ComposioClientKind` + the direct-mode reshapers scattered across `client.rs` | Proxy and direct differ in paths *and* response shape. One trait holds both differences so no caller above branches on mode. |
| `records::ConnectorRecordBatch` | the pipelines' direct writes to `MemoryClient` | A sync knows how to talk to Gmail; memory knows how to store things. Returning records means neither links the other. |
| the TinyBus interface | `composio::schemas` / `composio::ops` RPC controllers | The controllers were OpenHuman's RPC surface. The bus is the equivalent that a second host can also reach. |
| `ComposioHost` (phase 4) | `tinymemory_core::composio_host::ComposioHost` | Memory sync calls *out* to connectors. After the move that call is a bus call, and the trait becomes an adapter in the host rather than a global in memory. |

## Direction of dependency

`tinyconnectors` depends on `tinyconnectors-bus` and re-exports all of it, so
`tinyconnectors::ComposioConnection` and `tinyconnectors_bus::ComposioConnection`
are the same type. A parallel set of payload types for hosts would mean a
conversion at every call site that nothing checks.

After the migration:

- `openhuman` depends on `tinyconnectors-bus` and loads the module. Its
  `integrations::composio` module becomes a thin adapter over bus calls.
- `tinycortex` depends on `tinyconnectors-bus` and loads the module.
- `tinymemory` loses `ComposioHost`, `composio_host`, and the Composio sync
  pipelines; the host wires memory to connectors instead of memory reaching for
  Composio itself.

## What is deliberately out of scope

- **Changing any wire shape.** This is a move. A field that is wrong stays
  wrong until a separate change fixes it against a separate test.
- **A neutral connector abstraction over Composio.** There is one backend. An
  abstraction written against one implementation encodes that implementation's
  accidents as the interface. It gets written when there is a second backend to
  write it against.
- **Retry inside the client.** Rate-limit backoff is OAuth handoff policy and
  lives in `oauth`. A second retry in the client would multiply the two.

- **Mode *selection* inside the module.** The module routes; it does not decide.
  Reading a keychain, checking for a session, and preferring one mode over
  another are host concerns, and the module has no business acquiring a
  credential it was not given.

- **Direct-mode parity with the proxy.** The direct route implements what
  OpenHuman's direct client actually implements. Inventing v3 calls for the rest
  would mean shipping endpoint shapes nothing in either source repository
  verifies.
