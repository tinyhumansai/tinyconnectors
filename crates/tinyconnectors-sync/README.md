# tinyconnectors-sync

The provider registry, scope catalogs and scope preferences behind the
TinyConnectors module.

The name is historical: this crate used to hold the pipelines that pulled
records out of connected accounts (Gmail, Slack, Notion, GitHub, Linear,
`ClickUp`) on a schedule for a host to write into memory. That sync engine is
gone, along with its cursors, request budgets, dedupe sets and record
vocabulary. Connected accounts are reached through agent tool execution
(`Execute`) only. The crate keeps its name so downstream paths do not churn.

## What is here

| module     | what it holds                                                           |
| ---------- | ----------------------------------------------------------------------- |
| `scope`    | `ToolScope`, the curated-catalog types, `classify_unknown`, `toolkit_from_slug` |
| `prefs`    | `UserScopePref` and the `PrefsStore` seam it is persisted through       |
| `provider` | `ConnectorProvider`, `ProviderRegistry`, `ActionRunner`, `ProviderContext` |
| `toolkits` | the shipped providers and their curated catalogs, plus `default_registry` |

A provider answers three questions about a toolkit: its slug and description,
which actions are worth offering an agent, and who the connected account is
(`fetch_user_profile`). It reads nothing else.

## Persistence is the host's

`PrefsStore` is two methods over JSON, deliberately as small as it is: a seam
any wider would start describing a storage engine, and this crate must not
depend on one. `PREFS_NAMESPACE` and the lowercased toolkit key are durable and
pinned by tests; changing either strands every user's saved scope choices.

## Scope classification

Composio publishes sixty-odd actions per toolkit and most are noise for an
agent. A provider's curated catalog names the ones worth offering and tags each
with a `ToolScope`; the user's preference is checked against that tag.

Toolkits nobody has curated still need a scope, so `classify_unknown` derives
one from the action's verb. Destructive verbs are checked first: `MODIFY_LABELS`
is how Gmail archives, and classifying it as merely mutating would let a
read-and-write user have their mail moved by an action they never consented to.
Wrong in the cautious direction costs an action; wrong the other way costs mail.

## Toolkits

`gmail`, `github`, `notion`, `linear`, `clickup`, `slack` — each with its
curated catalog and its profile action. Every catalog is ported
action-for-action from the lists these toolkits were already curated against,
and a test asserts that each action belongs to its own toolkit, appears once,
and is scoped no less invasively than its verb.
