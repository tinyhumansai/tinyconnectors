# TinyConnectors

OAuth connector integrations for TinyHumans AI, shipped as an installable
TinyBus module. It links a user's third-party accounts, lists what those
accounts can do, runs actions against them, and subscribes to the webhooks they
emit — so `openhuman` and `tinycortex` gain connectors by loading a binary
rather than by compiling them in.

It is a two-crate cargo workspace. `crates/tinyconnectors-bus` is the wire
contract — member names, payload types, and the contract version, with no
transport and no behavior — and `crates/tinyconnectors` is the implementation,
built as both an `rlib` and the `cdylib` TinyBus loads. A host that only makes
calls depends on the contract crate alone and compiles neither the module nor
`tinybus` itself.

The contract re-exports the canonical `tinystoragedrivers_bus::StorageNeeds`
schema for the module's document and trigger-archive resources. Its TSD bus
dependency disables the optional TinyBus client, keeping host declarations
transport-free.

## Backends

Composio is the connector backend today, and the design does not assume it will
be the only one. Everything Composio-shaped is namespaced under `composio`; the
neutral parts — the OAuth handoff policy, the transport seam, the
error type — name it nowhere. A second backend arrives as a
sibling interface and object path, not as a rename of the first.

The `Composio`-prefixed payload types keep their names deliberately: they mirror
Composio's own response envelopes, and dressing them as a neutral abstraction
would be a lie the first time a second backend disagreed about a field.

## Two routes to Composio

Composio is reachable two ways, and the module implements both:

- **proxy** — through the TinyHumans backend, which owns the Composio API key,
  the billing margin, the toolkit allowlist, and webhook verification.
- **direct** — straight at `backend.composio.dev/api/v3` with a user-supplied
  `x-api-key`.

They differ in base URL, auth header, paths, *and response shape*, so a route
owns its paths and the translation of its responses. Nothing above the route
branches on which one is live.

**Choosing a route is the host's job.** Whether the user is signed in, whether
they supplied a key, and which the product prefers are decisions upstream of
this crate; the host states its choice in the module configuration blob.

The two are not equivalent and the module says so: direct mode has no per-user
allowlist to report, so `ListToolkits` returns a named refusal rather than an
empty list that would read as "you may connect nothing".

Direct mode also refuses the trigger members by default, because a webhook has
to arrive somewhere and the module has no endpoint. A host that owns a public
webhook endpoint sets `"webhooks": true` on its direct route (contract 1.22);
the members then run against Composio's trigger API as the configured entity
only, and the host verifies and decodes deliveries with `tinyconnectors-bus`'s
`webhook` feature.

## The module holds no connector credential

The credential is the host's to supply, in the configuration blob, and the
module reads one from nowhere else. It is never logged and never returned
through a member. A `base_url` that is not HTTPS or a genuine loopback address
is refused before any request is made — parsed, not prefix-matched, because
`http://127.0.0.1:8080@evil.com` resolves to `evil.com` and would carry the
credential header there.

## Concurrent caller routes

Contract 1.14 adds `RoutedRead`: one request carries a required proxy or direct
route and one of `ListToolkits`, `ListConnections`, `ListGithubRepos`,
`ListAvailableTriggers`, or `ListTriggers`. The module builds a local client
for that call. Concurrent callers can use different endpoints and credentials;
`Configure`, including sign-out, does not change an in-flight routed read.
Hosts must send this credential-bearing envelope with TinyBus confidential
transport. Route `Debug` output redacts credentials and private endpoints.
Direct mode retains its existing named refusals for proxy-only reads.

`Configure` remains available for existing single-route hosts. Its route is
module-wide, so separate configure-and-call sequences do not isolate concurrent
callers. `RoutedRead` does not read or change that route, preference stores,
profile caches, or trigger archives. Execution, filtered tools, provider profile
chains, and durable preferences/archive operations still need an independently
admitted state scope; this read contract does not provide one.

## Immutable routed-read-only module mode

Contract 1.15 adds the no-argument RuntimeMode handshake. To initialize a
module for concurrent callers without global state, supply exactly:

~~~json
{"runtime_mode":"routed_read_only"}
~~~

This immutable instance serves confidential RoutedRead, RuntimeMode, and
the static ListCapabilities / ListAgentReadyToolkits reads, plus framework
introspection. It allocates no client slot, preference store or archive.
All other connector members, including Configure, execution, providers,
preferences and archives, return typed unknown-method refusal before decoding
their arguments. A stored route or state directory is rejected during admission.

The handshake reports the **actual** live mode and contract version. A host
must require routed_read_only and a compatible 1.15 contract on its admitted
artifact before using this path. Sending an unknown configuration field to an
older module is insufficient: older releases may ignore it. Missing handshake,
classic mode, or unavailable confidential transport is a terminal refusal.

In restricted mode, ListCapabilities returns an empty matrix: native provider,
profile and execution methods are unavailable. ListAgentReadyToolkits remains a
pure catalog of compiled curated toolkit slugs, independent of account state.

Omitting runtime_mode retains classic initialization and its existing methods.
This mode grants no durable state scope; stateful cloud operations still require
a future admitted owning persistence contract.

## No sync engine

The module does not pull records out of connected accounts. Contract 1.12
removed the `Sync` member and the pipelines behind it (cursors, request
budgets, dedupe, per-toolkit paging). A host that wants data from a connected
account asks for it through `Execute`, one action at a time. See
[ADR 0002](docs/adr/0002-remove-the-sync-engine.md).

## Served surface

Interface `ai.tinyhumans.connectors.Composio` at
`/ai/tinyhumans/connectors/Composio`:

| member | takes | returns |
| --- | --- | --- |
| `ListToolkits` | — | `ComposioToolkitsResponse` |
| `ListConnections` | — | `ComposioConnectionsResponse` |
| `Authorize` | `ComposioAuthorizeRequest` | `ComposioAuthorizeResponse` |
| `DeleteConnection` | `ComposioDeleteConnectionRequest` | `ComposioDeleteResponse` |

`names::METHODS` lists what the module actually serves, never what is planned:
a constant for a member nothing answers is discovered by a host as a runtime
"unknown method". The remaining Composio operations arrive as additive minor
bumps of `CONTRACT_VERSION` — see
[`docs/plans/2026-08-30-connector-extraction.md`](docs/plans/2026-08-30-connector-extraction.md).

## Build And Test

```sh
git submodule update --init --recursive

cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
```

Run the bundled example, which drives the client over a stub backend and needs
no credential:

```sh
cargo run -p tinyconnectors --example basic
```

Verify a built module loads through the real TinyBus loader and serves the
members the contract declares:

```sh
cargo build -p tinyconnectors
cargo run -p tinyconnectors --example verify_module -- target/debug/libtinyconnectors.so
```

## Documentation

- [`AGENTS.md`](AGENTS.md) — how humans and agents work in this repository.
- [`docs/specs/`](docs/specs/) — accepted behavior and constraints.
- [`docs/plans/`](docs/plans/) — implementation-ordered plans.
- [`crates/tinyconnectors-bus/README.md`](crates/tinyconnectors-bus/README.md) —
  why the contract is its own crate.

## License

GPL-3.0-only. See [`LICENSE`](LICENSE).

## Static linking

Enable the `static-link` feature when compiling this module into a Rust host. It
exposes `tinybus_module::TINYBUS_MODULE_ABI_V1`,
`tinybus_module::tinybus_module_manifest_v1`, and
`tinybus_module::tinybus_module_init_v1` as Rust-addressable entries for the
TinyBus linked-module loader. The default build retains the native loadable
module exports. Build without `static-link` when packaging a `cdylib`.
