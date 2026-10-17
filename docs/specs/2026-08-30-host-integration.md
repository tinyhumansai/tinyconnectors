# Host integration

**Status:** accepted; the vocabulary swap and module record have landed.
Implements phase 5 of
[the extraction plan](../plans/2026-08-30-connector-extraction.md).

> **Superseded in part by [ADR 0002](../adr/0002-remove-the-sync-engine.md).**
> The `Sync` member, `ConnectorRecord*` types and sync pipelines described
> below were removed in contract 1.12. Every section describing them (including any phase marked landed that introduced them) is historical and no longer reflects the code; the remaining sections stand.

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
constructing a client and start choosing a blob. `state_dir` is accepted and
ignored: the module keeps its scope-preference store and the trigger archive in
the host's storage, reached over the storage bus
(`tinystoragedrivers-busproxy::connect_storage`), with the scope bound by the
host from the authenticated caller and the grant generated from `storage_schema()`. Without
served storage, history is unavailable and preferences are ephemeral. Until the preference migration in section 6
lands, the host still owns the user's actual preference.

The blob is also optional. A module loaded without one still answers the
capability members, so a signed-out user is not locked out of the question
"what could I connect?".

### Keeping the route current

A load-time blob alone is not enough, because the host's credential does not
stand still and the module is lazy. The common order of events is:

1. the application starts, signed out;
2. something touches a connector member, and the module loads — routeless;
3. the user signs in.

A route fixed at load would leave that user unable to reach Composio until they
restarted. Sign-out is the same problem reversed: a stale bearer answers 401 to
everything.

So the host calls `Configure` whenever its answer to "which route" changes —
sign-in, sign-out, a `composio.set_api_key`, a `composio.mode` switch. The
request carries the same tagged shape as the blob, so the host builds it once
and uses it for both. The module replaces the route unconditionally: which
route to use is the host's decision, and a `Configure` is an instruction rather
than a proposal.

Sign-out sends `{"route": "none"}` rather than nothing at all. A host that
could only *replace* a route would leave a revoked bearer in place, and every
member would answer 401 — which a user reads as a broken account rather than as
being signed out. It also keeps a credential from outliving the session that
supplied it, which matters on a shared machine.

`state_dir` is deliberately absent from the request. Storage is bound by the
host, not named by the module, so there is nothing for a later call to move.

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

### Triggers for a host with its own webhook endpoint

The proxy route gets webhook delivery from the backend. The direct route has no
backend, so by default it refuses every trigger member: a subscription whose
deliveries reach nobody is worse than none. A host that serves a public webhook
endpoint itself, and has set its Composio project's webhook URL to it, opts in
with `"webhooks": true` on the direct blob, the direct `Configure` request, or a
`RoutedRead` credential (contract 1.22). Absent means `false`.

With the flag on, the trigger members use Composio's trigger API with the
host's key, always as the configured `entity_id`, so one tenant of a shared key
never lists, subscribes on, or removes another's triggers:

| member | Composio call |
| --- | --- |
| `ListAvailableTriggers` | `GET /triggers_types?toolkit_slugs=…`; rows are `static` |
| `ListTriggers` | `GET /trigger_instances/active?user_ids=<entity>` |
| `CreateTrigger`, `EnableTrigger` | owner check, then `POST /trigger_instances/{slug}/upsert` |
| `DisableTrigger` | ownership lookup, then `DELETE /trigger_instances/manage/{id}` |

A connection is refused (`ConnectionNotOwned`) unless the entity is known to
own it: another user's account, a failed owner lookup and an account naming no
owner all fail closed. Removing a subscription that is not the entity's is
refused too (`TriggerNotOwned`). `ListGithubRepos` stays refused. The capability matrix's
`trigger_webhooks` is unchanged: it describes the compiled build, while
delivery depends on the route and the host.

The host's endpoint does the rest with `tinyconnectors-bus`'s `webhook` feature:

1. `webhook::verify(secret, headers, raw_body, now, DEFAULT_TOLERANCE_SECS)`
   on the raw body, before parsing. It checks the HMAC-SHA256 signature over
   `"{webhook-id}.{webhook-timestamp}.{body}"`, accepts any matching `v1`
   entry, and refuses a timestamp more than the tolerance from `now`. Answer a
   failure with a 4xx.
2. `webhook::decode(raw_body)` into a `ComposioTriggerEvent`, from the current
   or the legacy payload. `metadata.user_id` says which tenant it is for, and
   `metadata.trigger_id` and `metadata.connected_account_id` which subscription
   and account.
3. Optionally remember recent `webhook-id`s to refuse replays inside the
   tolerance window, then route the event to its tenant and, if the history
   member should report it, record it through the module's archive.

**Scope enforcement is the module's once the store moves (section 6).** `ListTools` hides what the user's
preference forbids and `Execute` refuses it. Do not re-filter host-side against
a separately stored preference — two sources of truth for a permission is how
one of them ends up stale and permissive.

## What changes for memory

Nothing. The `Sync` hand-off this section once described was removed in
contract 1.12 (see ADR 0002): OpenHuman no longer syncs Composio into memory.


## 6. The scope store is not moved yet

The module can filter a `ListTools` reply by the user's scope preference and
refuses an `Execute` the same preference forbids, reading both from its own
state directory.

OpenHuman's preference does not live there. It is two rows in the bound memory
driver's KV tier, written by the `composio.set_user_scopes` handler and read by
the agent-facing filter in `integrations::composio::tools`. Those rows are the
user's actual answer.

Until the store moves, the host asks for `apply_user_scopes: false` and keeps
applying the preference itself. The alternative — letting the module filter
against a store nobody has written to — reads as working and is wrong in the
one direction that matters: it would hide or allow actions by the module's
defaults rather than by what the user chose, and the two halves would disagree
the moment anyone touched the toggle.

Moving it is phase 4 work, and it is a data migration, not a flag: existing
users have rows in memory's KV that have to arrive in the module, or every
saved preference silently reverts to `read+write`.
