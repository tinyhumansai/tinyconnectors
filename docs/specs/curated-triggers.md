# Curated trigger catalog

**Status:** Implemented. **Owner:** TinyConnectors maintainers.
**Code:** `crates/tinyconnectors-sync/src/triggers/`.

## Problem

Composio offers hundreds of triggers across the toolkits a user can connect,
and most of them are noise for a personal assistant: sync cursors, every edit to
every calendar entry, events nobody plans around. Without a shared list, every
host rediscovers the same handful worth building on, re-verifies their slugs and
configuration keys, and spells "the new-email trigger" differently for each
mail toolkit.

## Goals and non-goals

Goals:

- Name the triggers worth building assistant features on, per toolkit, with the
  configuration keys Composio accepts for each.
- Key them by a toolkit-agnostic event, so a host asks for "new email" on
  whatever mail toolkit the user connected.
- Say how early each is worth building on, and which need the user to pick a
  resource first.

Non-goals:

- Creating, listing or deleting triggers. That is the module's
  `ComposioCreateTriggerRequest` family; this catalog only says which slugs to
  pass it.
- Gating which triggers a host may create. A slug outside the catalog is still
  valid at Composio.
- Describing any one product's features. Use cases describe what a trigger is
  good for in any personal assistant.

## Behavior

`tinyconnectors_sync::triggers` holds one table per toolkit family (Google,
Microsoft, messaging, GitHub, docs and tasks) and pure lookups over them:

| item                                    | answers                                                    |
| --------------------------------------- | ---------------------------------------------------------- |
| `curated_triggers()`                    | every row, toolkit by toolkit                              |
| `curated_trigger(slug)`                 | the row for a slug, case-insensitively                     |
| `curated_triggers_for_toolkit(toolkit)` | a toolkit's rows; aliases like `google_calendar` fold in   |
| `trigger_for_event(event, toolkit)`     | the trigger reporting `event` on the user's toolkit        |
| `toolkits_for_event(event)`             | the toolkits that can report `event`, each once            |

Each `CuratedTrigger` row carries the slug, its canonical toolkit, a
`TriggerEvent`, a `TriggerDelivery` (`webhook` or `poll`), a `TriggerTier`, the
required and optional configuration keys, and a one-sentence use case. The
three enums serialize in `snake_case` (`new_email`, `poll`, `per_resource`).

Tiers:

- **core** — build these first; they work account-wide with no configuration.
- **extended** — worth building once the core set works; also account-wide.
- **per_resource** — watches one repository, pull request, team or page, so a
  host must ask the user which before creating it.

Toolkit names resolve through the same alias folding as the curated tool
catalogs (`catalog_for_toolkit`), so an alias that reaches a tool catalog
reaches the same toolkit's triggers.

## The catalog

Configuration keys are listed in the code, spelled as Composio's config schema
spells them.

| event | toolkit | slug | delivery | tier | use case |
| --- | --- | --- | --- | --- | --- |
| meeting_starting_soon | googlecalendar | `GOOGLECALENDAR_EVENT_STARTING_SOON_TRIGGER` | poll | core | Brief the user before a meeting |
| meeting_starting_soon | googlesuper | `GOOGLESUPER_EVENT_STARTING_SOON_TRIGGER` | poll | core | Brief the user before a meeting |
| new_email | gmail | `GMAIL_NEW_GMAIL_MESSAGE` | poll | core | Alert on important mail, or mail from people the user cares about |
| new_email | googlesuper | `GOOGLESUPER_NEW_MESSAGE` | poll | core | Same as Gmail |
| new_email | outlook | `OUTLOOK_MESSAGE_TRIGGER` | webhook | core | Same as Gmail |
| calendar_event_created | googlecalendar | `GOOGLECALENDAR_GOOGLE_CALENDAR_EVENT_CREATED_TRIGGER` | poll | core | New invite or booking: check conflicts and prepare |
| calendar_event_created | googlesuper | `GOOGLESUPER_GOOGLE_CALENDAR_EVENT_CREATED_TRIGGER` | poll | core | Same as Google Calendar |
| calendar_event_created | outlook | `OUTLOOK_EVENT_TRIGGER` | webhook | core | Same as Google Calendar |
| direct_message | slack | `SLACK_DIRECT_MESSAGE_RECEIVED` | webhook | core | Surface DMs while the user is away from Slack |
| calendar_event_canceled | googlecalendar | `GOOGLECALENDAR_EVENT_CANCELED_DELETED_TRIGGER` | poll | extended | A meeting was canceled; the time is free |
| calendar_event_canceled | googlesuper | `GOOGLESUPER_EVENT_CANCELED_DELETED_TRIGGER` | poll | extended | Same as Google Calendar |
| attendee_response_changed | googlecalendar | `GOOGLECALENDAR_ATTENDEE_RESPONSE_CHANGED_TRIGGER` | poll | extended | Someone accepted or declined the user's meeting |
| attendee_response_changed | googlesuper | `GOOGLESUPER_ATTENDEE_RESPONSE_CHANGED_TRIGGER` | poll | extended | Same as Google Calendar |
| email_sent | gmail | `GMAIL_EMAIL_SENT_TRIGGER` | poll | extended | Follow-up reminder when a sent mail gets no reply |
| email_sent | googlesuper | `GOOGLESUPER_EMAIL_SENT_TRIGGER` | poll | extended | Same as Gmail |
| email_sent | outlook | `OUTLOOK_SENT_MESSAGE_TRIGGER` | webhook | extended | Same as Gmail |
| channel_message | slack | `SLACK_CHANNEL_MESSAGE_RECEIVED` | webhook | extended | Watch a channel for mentions or keywords |
| issue_assigned_to_me | github | `GITHUB_ISSUE_ASSIGNED_TO_ME_TRIGGER` | poll | extended | An issue landed on the user's plate |
| page_comment | notion | `NOTION_COMMENT_CREATED` | webhook | extended | Comments on the user's pages that may need an answer |
| file_shared | googledrive | `GOOGLEDRIVE_FILE_SHARED_PERMISSIONS_ADDED` | poll | extended | A file was shared with the user; offer a summary |
| file_shared | googlesuper | `GOOGLESUPER_FILE_SHARED_PERMISSIONS_ADDED` | poll | extended | Same as Google Drive |
| document_comment | googledrive | `GOOGLEDRIVE_COMMENT_ADDED_TRIGGER` | poll | extended | Comments on the user's documents that may need an answer |
| document_comment | googlesuper | `GOOGLESUPER_COMMENT_ADDED_TRIGGER` | poll | extended | Same as Google Drive |
| reaction_added | slack | `SLACK_REACTION_ADDED` | webhook | extended | A chosen reaction becomes a task or reminder |
| pull_request_activity | github | `GITHUB_PULL_REQUEST_EVENT` | webhook | per_resource | Follow PRs on a repository (`owner`, `repo`) |
| repository_notification | github | `GITHUB_REPOSITORY_NOTIFICATION_RECEIVED_TRIGGER` | poll | per_resource | Relay notifications for a repository (`owner`, `repo`) |
| pull_request_review_submitted | github | `GITHUB_PULL_REQUEST_REVIEW_SUBMITTED_TRIGGER` | poll | per_resource | A review landed on a PR (`owner`, `repo`, `pull_number`) |
| issue_created | linear | `LINEAR_ISSUE_CREATED_TRIGGER` | webhook | per_resource | New issues in a team (`team_id`) |
| issue_updated | linear | `LINEAR_ISSUE_UPDATED_TRIGGER` | webhook | per_resource | Status, priority and assignee changes (`team_id`) |
| issue_comment | linear | `LINEAR_COMMENT_EVENT_TRIGGER` | webhook | per_resource | Comments on a team's issues (`team_id`) |
| page_created | notion | `NOTION_PAGE_CREATED` | webhook | per_resource | New pages under a watched parent (`parent_page_id`) |

## Notes

- **Verification.** Every slug and configuration key was checked against
  Composio with `composio dev triggers info <SLUG>` on 2026-10-15. Re-run that
  command for any row before changing it; `fixtures/curated-triggers.json` in
  the module pins the result, and a test compares the tables against it.
- **`googlesuper` mirrors Gmail, Calendar and Drive.** It is Composio's single
  Google connection; each of its rows matches the per-product trigger's event,
  delivery, tier and configuration. A host picks whichever toolkit the user
  connected, which is what `trigger_for_event` does.
- **Poll triggers take an `interval` in minutes**, so their latency is up to one
  interval. Webhook triggers take none; a test pins both directions.
- **Notion's page-created trigger is per-resource without a required key.**
  Composio lists `parent_page_id` as optional, and the catalog records it that
  way, but without a parent the trigger fires for every page in the workspace.
  Hosts should treat `parent_page_id` as the resource to ask for.

## Left out, and why

- **Google Calendar's sync and event-updated triggers** fire on every edit to
  every event, which is noise; created, canceled and attendee-response cover
  what a user notices.
- **Figma** had no trigger types in Composio's catalog when this list was
  checked.
- **Outlook contact-added** is low value for an assistant.

## Invariants

- Every slug starts with its toolkit's upper-cased slug and a `_`, and every
  toolkit is canonical (what `canonical_toolkit` returns for it).
- Slugs are unique, and a toolkit has at most one trigger per event.
- Every `TriggerEvent` has at least one trigger.
- Only per-resource rows have required configuration; Notion's page-created
  trigger is the one per-resource row whose resource key is optional.

## Acceptance criteria

- `trigger_for_event(NewEmail, "outlook")` is `OUTLOOK_MESSAGE_TRIGGER`.
- `trigger_for_event(MeetingStartingSoon, "googlesuper")` is
  `GOOGLESUPER_EVENT_STARTING_SOON_TRIGGER`.
- `curated_triggers_for_toolkit("google_calendar")` returns the four
  `GOOGLECALENDAR_*` rows.
- The wire forms of the three enums are pinned by unit tests.

## Open questions

None.
