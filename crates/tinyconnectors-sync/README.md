# tinyconnectors-sync

Pulling records out of connected accounts, for a host to ingest.

A sync run reads a user's Gmail, Slack, or Notion through a connector and
produces `tinyconnectors_bus::ConnectorRecordBatch`. It stores nothing. The host
takes the batch and writes it to memory over memory's own bus API.

## No memory dependency, on purpose

These pipelines used to live inside the memory system and call its store
directly, which is why they could not be moved without taking half of memory
with them. Returning records cuts that: a pipeline knows how to talk to Gmail,
memory knows how to store things, and neither links the other. The manifest
carries no memory crate and must not gain one.

## What a run does need to remember

Three things it cannot recompute:

| | why |
| --- | --- |
| a cursor | so the next run resumes instead of re-reading a whole mailbox |
| seen ids and versions | so an unchanged item is not re-ingested as new |
| today's request budget | so a run that goes wrong cannot spend the day's allowance in a loop |

All three go through `state::SyncStateStore` — two methods over JSON, which the
host implements against whatever it already has. That is deliberately as small
as it is: a seam any wider would start describing a storage engine, and this
crate would be depending on one again.

`STATE_NAMESPACE` and the `toolkit:connection_id` key are durable and pinned by
tests. Changing either strands every user's cursor, and every connection
re-reads its entire history on the next run.

## Scope classification

Composio publishes sixty-odd actions per toolkit and most are noise for an
agent. A provider's curated catalog names the ones worth offering and tags each
with a `ToolScope`; the user's preference is checked against that tag.

Toolkits nobody has curated still need a scope, so `classify_unknown` derives
one from the action's verb. Destructive verbs are checked first — `MODIFY_LABELS`
is how Gmail archives, and classifying it as merely mutating would let a
read-and-write user have their mail moved by an action they never consented to.
Wrong in the cautious direction costs an action; wrong the other way costs mail.

## Status

Foundations only so far: `scope` and `state`. The provider registry, the
per-provider catalogs, the pipelines, and the record post-processing are still
being migrated — see
[`docs/plans/2026-08-30-connector-extraction.md`](../../docs/plans/2026-08-30-connector-extraction.md).
