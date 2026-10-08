# 2. Remove the sync engine

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

The module carried a connector sync engine: per-toolkit pipelines that paged
through a connected Gmail, Slack, Notion, GitHub, Linear or ClickUp account on a
schedule and returned `ConnectorRecordBatch` for the host to write into memory.
It needed its own durable state (cursors, seen-id sets, a daily request budget),
a record vocabulary on the wire, and roughly 8k lines of paging and
post-processing code. OpenHuman has dropped Composio-to-memory syncing, so
nothing calls it.

## Decision

Delete the engine and everything only it used:

- the `Sync` member, `names::methods::SYNC`, and the `records` module of
  `tinyconnectors-bus` (`ConnectorRecord`, `ConnectorRecordBatch`,
  `RecordSender`, `SyncEvent`, `SyncStage`, `ConnectorSyncRequest`,
  `ConnectorSyncResponse`);
- the sync fields of `ComposioCapability` (`initial_sync`, `periodic_sync`,
  `sync_interval_secs`, `memory_ingest`);
- in `tinyconnectors-sync`: `pipeline`, `clean`, `state` (`SyncStateStore`,
  `SyncState`, `DailyBudget`), `SyncReason`, `SyncLimits`, the provider
  `fetch_page` / `can_sync` / `sync_interval_secs` hooks, and every toolkit's
  page spec and post-processing (including the Slack walk);
- in `tinyconnectors`: `FileStateStore`.

Keep what tool execution and connection management still need: the scope
catalogs and classification, the per-toolkit action catalogs, the provider
registry (capabilities, agent-ready toolkits, identity fetch), `ActionRunner`,
`toolkit_from_slug`, and the user scope preferences. Preferences were persisted
through the sync state seam, so that seam shrinks to `PrefsStore`, and
`FileStateStore` becomes `FilePrefsStore` with the same on-disk layout
(`<state_dir>/composio-user-scopes/<toolkit>.json`), so saved choices keep
reading. The `tinyconnectors-sync` crate keeps its name to avoid churning
downstream paths.

`CONTRACT_VERSION` moves from 1.11 to 1.12. Removing a member is strictly a
major change, but the only host that called `Sync` drops it in the same
change and pins the module release it binds to, so a minor bump keeps the 1.x
line without stranding any caller.

## Consequences

- A host built against contract 1.12 no longer binds to a 1.11 module; it must
  drop its `Sync` call sites and stop reading the removed capability fields.
- Old `sync-state` rows under a host's `state_dir` are orphaned and can be
  deleted; the module never reads them.
- The module cannot ingest account data on its own. Reading an account is an
  `Execute` call the caller makes deliberately.
