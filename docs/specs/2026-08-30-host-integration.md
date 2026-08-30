# Host integration

**Status:** accepted; the vocabulary swap and module record have landed.
Implements phase 5 of
[the extraction plan](../plans/2026-08-30-connector-extraction.md).

## What a host has to do

Three things, in order. Only the first is optional.

1. Depend on `tinyconnectors-bus` to name the payload types and the members.
2. Load `tinyconnectors` as a `TinyBus` module, with a config blob.
3. Replace its own Composio client calls with bus calls.

## 1. The vocabulary

`tinyconnectors-bus` has two dependencies, both pure Rust, and no transport. A
host that only makes calls links it and compiles neither the module nor
`tinybus`.

OpenHuman vendors its modules as git submodules under `vendor/`, so:

```sh
git submodule add https://github.com/tinyhumansai/tinyconnectors vendor/tinyconnectors
```

```toml
tinyconnectors-bus = { path = "vendor/tinyconnectors/crates/tinyconnectors-bus" }
```

The payload types currently reached through `tinymemory_api::host::composio`
come from here instead. They are the same shapes — that crate's copy was
written from the same backend envelopes — so this is an import change, not a
conversion. Do it in its own commit, before anything else: it is the change
that stops the two definitions drifting, and it is the only one that can land
without a release.

## 2. Loading the module

OpenHuman's `ModuleRecord` carries a release URL and a SHA-256 per platform
archive, and the loader verifies the digest before initializing. Those digests
come from a real release: run the `release.yml` workflow
(`workflow_dispatch`), then read them off the published `checksum.toml`. This
is done for v0.3.0.

**The module loads with no configuration**, so `LoadPolicy::Lazy` is safe even
for a signed-out user: the capability members answer, and a member that needs a
route says which one is missing when it is called.

The record follows the shape of `TINYMCP` in
`src/openhuman/modules/registry.rs`:

```rust
const TINYCONNECTORS: ModuleRecord = ModuleRecord {
    id: "tinyconnectors",
    description: "OAuth connector integrations: accounts, actions, triggers, and sync",
    bus_name: "ai.tinyhumans.connectors.Composio",
    object_path: "/ai/tinyhumans/connectors/Composio",
    version: "<the released version>",
    release_url: "https://github.com/tinyhumansai/tinyconnectors/releases/tag/v<version>",
    assets: &[/* one per platform, digests from checksum.toml */],
    load: LoadPolicy::Lazy,
};
```

`Lazy`, because a user with no connected accounts should not pay to load it.

### The config blob

The host builds this from what it already knows, and it is the **only** way the
module gets a credential:

```json
{ "route": "proxy",  "base_url": "…", "auth_token": "…", "state_dir": "…" }
{ "route": "direct", "api_key": "…", "entity_id": "default", "state_dir": "…" }
```

`config.composio.mode` and the keychain lookup in
`security::credentials::get_composio_api_key` stay in OpenHuman. They stop
constructing a client and start choosing a blob. `state_dir` should be the
host's existing state directory — the module keeps its sync cursors and the
trigger archive there.

## 3. Replacing the calls

`integrations::composio` keeps its module path — it is referenced across the
crate — and becomes an adapter. Per file:

| OpenHuman | becomes |
| --- | --- |
| `client.rs`, `execute_*.rs`, `error_mapping.rs`, `auth_retry.rs`, `oauth_handoff.rs`, `trigger_history.rs`, `identity.rs` | delete; the module owns them |
| `schemas.rs` / `ops.rs` | keep. These are OpenHuman's own RPC surface; the handlers become bus calls |
| `tools.rs`, `tools/direct.rs`, `action_tool.rs` | keep. Model-facing agent tools belong to whoever runs an agent loop |
| `bus.rs` | keep. Webhooks still arrive over the socket transport |

### Three things not to lose

**Egress enforcement.** `execute_tool` currently calls
`security::egress::enforce_egress` and `emit_external_transfer` before every
Composio call. The module does **not** do this and must not: it is host policy
about the user's data. Apply it in the adapter, before the bus call. Losing it
means a Composio action ships arguments off-device under local-only mode.

**`ComposioTriggerSubscriber`.** The backend HMAC-verifies webhooks and fans
them out over the user's sockets. The module has no socket, so this stays
exactly where it is. Feed each delivery to `ListTriggerHistory`'s archive by
calling the module, so the history member has something to report.

**Scope enforcement is now the module's.** `ListTools` hides what the user's
preference forbids and `Execute` refuses it. Do not re-filter host-side against
a separately stored preference — two sources of truth for a permission is how
one of them ends up stale and permissive.

## What changes for memory

Nothing, until phase 4. `tinymemory` keeps its Composio sync until the host
calls `Sync` on the module instead, and the sync returns records the host writes
to memory over memory's own API. Phase 4 then deletes what nothing calls —
see the ordering note at the top of that phase.
