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

## Backends

Composio is the connector backend today, and the design does not assume it will
be the only one. Everything Composio-shaped is namespaced under `composio`; the
neutral parts — the OAuth handoff policy, the transport seam, the record
vocabulary, the error type — name it nowhere. A second backend arrives as a
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

## The module holds no connector credential

The credential is the host's to supply, in the configuration blob, and the
module reads one from nowhere else. It is never logged and never returned
through a member. A `base_url` that is not HTTPS or a genuine loopback address
is refused before any request is made — parsed, not prefix-matched, because
`http://127.0.0.1:8080@evil.com` resolves to `evil.com` and would carry the
credential header there.

## What a sync emits

Connector sync does not write memory. It returns `ConnectorRecordBatch`, the
host hands it to the memory engine over memory's own bus API, and neither side
links the other. `ConnectorRecord`'s wire shape is memory's ingestion vocabulary
exactly — asserted in a test — so the join needs no translation step.

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
