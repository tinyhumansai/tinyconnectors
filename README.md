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
neutral parts — the OAuth handoff policy, the transport seam, the error type —
name it nowhere. A second backend arrives as a sibling interface and object
path, not as a rename of the first.

The `Composio`-prefixed payload types keep their names deliberately: they mirror
Composio's own response envelopes, and dressing them as a neutral abstraction
would be a lie the first time a second backend disagreed about a field.

## The module holds no connector credential

It never calls Composio. Every request goes through a backend that owns the
Composio API key, the billing margin, the toolkit allowlist, and the HMAC
verification of inbound webhooks. That backend authenticates the *user*, whose
credential belongs to the host — so the module is handed a `base_url` and an
`auth_token` in its configuration blob at load time and can reach nothing else.

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
