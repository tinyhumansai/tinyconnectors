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

## Phase 2 — the rest of the Composio operations ✅ landed

Source: `openhuman/src/openhuman/integrations/composio/` — `client.rs`,
`ops/`, `catalog.rs`, `trigger_history.rs`, `identity.rs`, `task_window.rs`,
`contract_gate.rs`, `error_mapping.rs`, `execute_prepare.rs`,
`execute_dispatch.rs`, `googlecalendar_args.rs`.

All nineteen landed, contract `(1, 4)`. Grouped by what they needed:

1. **Tools and execute** — `ListTools`, `Execute`. ✅ **landed**, contract
   `(1, 1)`. `crates/tinyconnectors/src/execute/` holds the whole pipeline:
   `prepare` (argument normalization and validation), `classify` (failure
   classes and their messages), `retry` (both policies). Both routes implement
   `list_tools` and `execute`; the direct route reshapes v3 into the proxy's
   envelope.

   Two things worth knowing for the groups that follow. The upstream had
   **three** retry layers that could stack — an in-client retry, a wrapper
   around a non-retrying primitive that existed only to avoid the first, and a
   rate-limit loop — which its own comments record as issuing up to four calls
   per logical retry. Here the policy lives in one place. And **egress
   enforcement did not move**: OpenHuman refuses outbound tool calls under
   local-only mode and discloses every external transfer. That is host policy
   about the user's data, applied before the bus call.
2. **Triggers** — ✅ seven members. The JSONL archive came too, as
   `crates/tinyconnectors/src/triggers/`. **Every trigger member is refused on
   the direct route**: a trigger is a webhook, and a webhook has to arrive
   somewhere. The proxy backend HMAC-verifies deliveries and fans them out over
   the user's sockets; the module has no socket and no public endpoint, so a
   direct-mode subscription would be created and then deliver to nobody.
3. **Identity and profile** — ✅ answered from the provider registry.
   `RefreshAllIdentities` reports per-connection failures alongside the
   successes rather than failing wholesale: a refresh exists to find the broken
   connections, so one of them must not hide the rest.
4. **Capabilities and catalog** — ✅ derived from the registry, so they answer
   with no session and no network, as intended.
5. **Scopes** — ✅ and, more importantly, **enforced**. The module owns the
   preference in its own state store, `ListTools` hides what it forbids, and
   `Execute` refuses it. Passing the preference in from the caller was the
   obvious alternative and the wrong one: a restriction a caller can opt out of
   is a suggestion.
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

## Phase 3 — the sync pipelines ✅ landed

Source: `tinymemory-core/src/sync/composio/` (~12k),
`tinymemory-core/src/sync/pipelines/composio/` (~6.5k), and
`tinymemory-sync/` (~3.3k).

Lands as a new workspace member, `crates/tinyconnectors-sync`, so the module
crate does not grow a memory dependency:

1. **Foundations** — ✅ **landed**. `crates/tinyconnectors-sync` exists with
   `scope` (`ToolScope`, `CuratedTool`, `classify_unknown`, `toolkit_from_slug`)
   and `state` (`SyncStateStore`, `SyncState`, `DailyBudget`). No memory
   dependency, and its manifest says so.

   **The memory-read seam turned out to be smaller than feared.** Two findings
   from reading the source rather than the call graph:

   - `SyncStateStore` already exists as an engine-neutral KV trait — two methods
     over JSON. It *is* the host-supplied input, and it moved as-is.
   - `ProviderContext::memory_client()` has **zero callers** in the entire sync
     tree. The direct memory coupling I flagged as the riskiest thing in this
     phase is dead API.

   So the only real coupling left is `pipelines::host::run_composio_connection`,
   which is the call the record-returning shape replaces anyway.

2. **Provider registry and traits** — `providers/traits.rs`,
   `providers/registry.rs`, `providers/types.rs`, `user_scopes.rs`.
   `ProviderContext` sheds `memory_client` (dead) and gains a
   `&dyn SyncStateStore`.
3. **Per-provider catalogs** — ✅ 159 curated actions across five toolkits,
   ported action-for-action.

   Porting them found a real bug in the scope heuristic, because the catalogs
   disagreed with it: `GMAIL_LIST_DRAFTS` is a read that a substring scan calls
   a write, since the *noun* is `DRAFTS`. `classify_unknown` now weighs the
   leading verb, so a read-only user is no longer denied a listing.
4. **Pipelines and orchestrator** — `pipelines/composio/`, including the
   per-provider modules and `page_size.rs`. Each pipeline's `MemoryClient`
   writes become records appended to a batch; its progress logging becomes a
   `SyncEvent`; its paging loop becomes `cursor` + `complete` on the batch, so
   the host drives resumption.
5. **Record post-processing** — still to come: `tinymemory-sync`'s
   `gmail_post_process`, `slack_post_process`, `email_clean`, `email_markdown`
   and the per-provider normalizers. Records currently carry the provider's
   text as-is, which is correct but noisier than it needs to be — HTML mail in
   particular. Pure functions over payloads, so they port almost unchanged.

**Settled: pipelines return records.** A pipeline produces
`ConnectorRecordBatch` and returns it; the host hands it to the memory engine
over memory's own bus API. `crates/tinyconnectors-sync` therefore takes
`tinyconnectors-bus` and *no memory dependency at all* — which is what makes
this phase a move rather than a rewrite.

A `Sync` member on the bus emits batches. It is added when the first pipeline
lands, as a minor contract bump.

---

## Phase 4 — retire `ComposioHost` from tinymemory

> **Ordering correction.** This plan originally put phase 4 before phase 5.
> That is wrong, and attempting it showed why: deleting the Composio tree from
> `tinymemory-core` does not compile, because five things outside that tree
> still call into it —
>
> | caller | what it calls |
> | --- | --- |
> | `engine/sync.rs` | `run_composio_connection`, `run_gmail_backfill`, `run_slack_search_backfill`, `syncable_composio_toolkits` |
> | `sources/reconcile.rs` | `scan_active_sync_targets` |
> | `sources/sync.rs` | `ComposioUsage` |
> | `store/entities.rs` | `is_self_identity_any_toolkit`, `IdentityKind` |
> | `sync/workspace/periodic.rs` | the Composio scheduler |
> | `sources/readers/mod.rs` | `ComposioReader` for `SourceKind::Composio` |
>
> These are behaviours to be **replaced**, not removed: something still has to
> reconcile a user's Composio sources and run their Gmail backfill. After phase
> 5 that something is the host, calling the module. Until then, deleting them
> deletes the feature.
>
> So: **phase 5 first, then phase 4.** Phase 4 then becomes what it was always
> supposed to be — a pure deletion of code nothing calls.
>
> One detail worth carrying: `SourceKind::Composio` should *stay*. Memory still
> holds such sources; it just no longer reads them. `sources::readers::reader_for`
> becomes `-> Option<Box<dyn SourceReader>>` returning `None` for that kind —
> an `Option` rather than a reader that always errors, because "memory does not
> read this kind" is a routing fact a caller acts on, while a perpetually
> failing reader looks like a broken integration.


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

Phases 2 and 3 are independent and both landed. **Phase 5 must precede phase
4** — see the correction at the top of phase 4.

Both open design questions are settled — records out rather than memory writes,
and routing in the module with selection in the host — and the memory-read seam
turned out to already exist as `SyncStateStore`, with the one type that looked
like a hard coupling having no callers at all.

What remains riskiest is now phase 4's deletion step: until the payload types
are removed from `tinymemory-api`, they exist in two places and can drift. The
sooner phase 3 lands far enough to allow that, the shorter that window.
