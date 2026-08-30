# Connector extraction — implementation plan

Implements [`docs/specs/2026-08-30-connector-extraction.md`](../specs/2026-08-30-connector-extraction.md).

Each phase lands on its own branch and leaves the four contract commands green.
Phases are ordered by dependency: nothing in a later phase can be written
before the seam an earlier one introduces.

---

## Phase 1 — workspace and wire contract ✅ landed

Renamed `template` / `template-bus` to `tinyconnectors` /
`tinyconnectors-bus`, retired the placeholder `greeting` module, and moved the
payload vocabulary out of `tinymemory-api::host::composio`.

- `crates/tinyconnectors-bus/src/composio/` — six payload families
  (`toolkits`, `connections`, `tools`, `execute`, `triggers`, `github`), 29
  types, each family pinning its serde form in `test.rs`.
- `crates/tinyconnectors-bus/src/names/` — interface
  `ai.tinyhumans.connectors.Composio` at `/ai/tinyhumans/connectors/Composio`.
- `crates/tinyconnectors-bus/src/records/` — the ingestion vocabulary a sync
  emits: `ConnectorRecord`, `ConnectorRecordBatch`, `SyncStage`, `SyncEvent`.
- `crates/tinyconnectors/src/client/` — the `Transport` seam, the `Route` seam
  with `ProxyRoute` and `DirectRoute`, `ComposioClient`, and `HttpTransport`.
- `crates/tinyconnectors/src/oauth/` — Meta rate-limit policy and the authorize
  backoff.
- `crates/tinyconnectors/src/tinybus_module/` — serves `ListToolkits`,
  `ListConnections`, `Authorize`, `DeleteConnection`.

**Verified:** 121 tests; `cargo run --example verify_module` loads the built
`cdylib` through the real TinyBus loader and matches its manifest members
against `names::METHODS`.

---

## Phase 2 — the rest of the Composio operations

Source: `openhuman/src/openhuman/integrations/composio/` — `client.rs`,
`ops/`, `catalog.rs`, `trigger_history.rs`, `identity.rs`, `task_window.rs`,
`contract_gate.rs`, `error_mapping.rs`, `execute_prepare.rs`,
`execute_dispatch.rs`, `googlecalendar_args.rs`.

Nineteen members, each a `CONTRACT_VERSION` minor bump as it lands. Grouped by
what they need, in dependency order:

1. **Tools and execute** — `ListTools`, `Execute`. Brings
   `execute_prepare`/`execute_dispatch`, `error_mapping`, and the
   `googlecalendar_args` normalization. Note the post-OAuth readiness retry in
   `client.rs`: Composio's gateway reports "connection error, try to
   authenticate" for a window after a handoff completes, and the existing code
   retries once after ten seconds.
2. **Triggers** — `ListAvailableTriggers`, `ListTriggers`, `EnableTrigger`,
   `DisableTrigger`, `CreateTrigger`, `ListGithubRepos`, `ListTriggerHistory`.
   The JSONL archive in `trigger_history.rs` moves with them.
3. **Identity and profile** — `GetUserProfile`, `RefreshAllIdentities`.
   Populates the `account_email` / `workspace` / `username` hints that let a
   picker distinguish two connections of the same toolkit.
4. **Capabilities and catalog** — `ListCapabilities`,
   `ListAgentReadyToolkits`. These describe the compiled build, so they must
   answer without a session; keep them off the transport.
5. **Scopes** — `GetUserScopes`, `SetUserScopes`.
6. **Mode** — `GetMode`, `SetApiKey`, `ClearApiKey`. **Settled:** these stay
   host-side. The module routes but does not select, and it must not acquire a
   credential it was not given, so reading the keychain and writing a key are
   the host's. `GetMode` has no module member because the host already knows
   which route it configured; `ComposioClient::route_name` covers diagnostics.
   Changing route means reloading the module with a different config blob.

Direct-mode coverage grows with each group. Where Composio's v3 API genuinely
has no equivalent, the route returns `Error::UnsupportedByRoute` rather than an
invented call — the same rule `ListToolkits` and `DeleteConnection` already
follow.

**Not moving:** `tools.rs`, `tools/direct.rs`, `action_tool.rs`. Those are
model-facing agent tools; they belong to whichever host runs an agent loop and
should call the bus members instead.

---

## Phase 3 — the sync pipelines

Source: `tinymemory-core/src/sync/composio/` (~12k),
`tinymemory-core/src/sync/pipelines/composio/` (~6.5k), and
`tinymemory-sync/` (~3.3k).

Lands as a new workspace member, `crates/tinyconnectors-sync`, so the module
crate does not grow a memory dependency:

1. **Provider registry and traits** — `providers/traits.rs`,
   `providers/registry.rs`, `providers/types.rs`, `tool_scope.rs`,
   `user_scopes.rs`, `sync_state.rs`.
2. **Per-provider catalogs** — `providers/catalogs*.rs` and the
   `gmail` / `slack` / `github` / `notion` / `linear` / `clickup` directories.
3. **Pipelines and orchestrator** — `pipelines/composio/`, including the
   per-provider modules and `page_size.rs`.
4. **Record post-processing** — the `tinymemory-sync` crate's
   `gmail_post_process`, `slack_post_process`, `email_clean`,
   `email_markdown`, and the per-provider normalizers.

**Settled: pipelines return records.** A pipeline produces
`ConnectorRecordBatch` and returns it; the host hands it to the memory engine
over memory's own bus API. `crates/tinyconnectors-sync` therefore takes
`tinyconnectors-bus` and *no memory dependency at all* — which is what makes
this phase a move rather than a rewrite.

Concretely, per pipeline:

- Replace every `MemoryClient` write with a record appended to the batch.
- Replace progress logging with a `SyncEvent`.
- Paging state becomes `ConnectorRecordBatch::cursor` and `complete`, so a run
  is resumable by the host rather than by a pipeline-local loop.

The one thing to watch: some pipelines currently *read* memory to decide what to
re-sync (`sync_state.rs`, the diff logic). Those reads become an input the host
supplies to the run, not a call the pipeline makes. Design that input before
porting the first provider — it is the only remaining coupling.

A `Sync` member on the bus emits batches. It is added when the first pipeline
lands, as a minor contract bump.

---

## Phase 4 — retire `ComposioHost` from tinymemory

`tinymemory-core/src/composio_host.rs` is a global `RwLock<Option<Arc<dyn
ComposioHost>>>` that memory sync reaches through to call Composio. After
phase 3 there is nothing in tinymemory that needs it.

1. Delete the Composio sync pipelines and `sources/readers/composio.rs` from
   `tinymemory-core`.
2. Delete `composio_host.rs` and the `ComposioHost` trait.
3. Remove the payload types from `tinymemory-api/src/host/composio.rs`. They
   now live in `tinyconnectors-bus`; anything in tinymemory that still names
   one takes that crate.
4. Remove the Composio entries from `tinymemory-sources` and its registry.

This phase is where the migration either pays off or does not. Until it lands,
the types exist in two places and can drift.

---

## Phase 5 — re-import into the hosts

### `openhuman`

1. Depend on `tinyconnectors-bus`; load `tinyconnectors` as a module, passing
   the route it selected as the config blob — `{"route": "proxy", …}` for a
   signed-in user, `{"route": "direct", …}` for one with their own key. The
   existing `composio.mode` config and the keychain lookup stay in OpenHuman;
   they now choose a blob instead of constructing a client.
2. Replace `integrations::composio::client` with bus calls. Keep the
   `integrations::composio` module path — it is referenced across the crate —
   but reduce it to an adapter.
3. Keep the RPC controllers in `schemas.rs` / `ops.rs`: they are OpenHuman's
   own public surface. Their handlers become bus calls.
4. `ComposioTriggerSubscriber` keeps listening for
   `DomainEvent::ComposioTriggerReceived`; the socket transport still delivers
   webhooks, since the backend fans them out over the user's sockets and the
   module has no socket.

### `tinycortex`

Depend on `tinyconnectors-bus` and load the module. Narrower than OpenHuman —
it needs connections and execute, not the trigger or catalog surface.

---

## Sequencing note

Phases 2 and 3 are independent and can run in parallel; both must land before
phase 4, and phase 4 before phase 5 for either host.

Both open design questions are now settled — records out rather than memory
writes, and routing in the module with selection in the host. What remains
riskiest in phase 3 is the pipelines that *read* memory to decide what to
re-sync: that input has to be designed before the first provider is ported,
because retrofitting it means touching every pipeline again.
