//! Curated triggers — Google toolkits: Gmail, `GoogleCalendar`,
//! `GoogleDrive`, and `GoogleSuper`, which mirrors all three under one
//! connection.
//!
//! Every Google trigger polls, so each takes an `interval` in minutes.

use super::types::{CuratedTrigger, TriggerDelivery, TriggerEvent, TriggerTier};

// Configuration and use cases shared by a toolkit's trigger and its
// `googlesuper` mirror, which Composio configures identically.
const STARTING_SOON_CONFIG: &[&str] = &[
    "calendarId",
    "countdownWindowMinutes",
    "includeAllDay",
    "interval",
    "minutesBeforeStart",
];
const STARTING_SOON_USE: &str = "Brief the user before a meeting: who is attending, what was said \
     last time, and what to prepare.";

const NEW_MESSAGE_CONFIG: &[&str] = &["interval", "labelIds", "query", "userId"];
const NEW_MESSAGE_USE: &str =
    "Alert the user to important mail, or to mail from people they care about.";

const EMAIL_SENT_CONFIG: &[&str] = &["interval", "query", "userId"];
const EMAIL_SENT_USE: &str = "Remind the user to follow up when a mail they sent gets no reply.";

const EVENT_CREATED_CONFIG: &[&str] = &["calendarId", "interval", "showDeleted"];
const EVENT_CREATED_USE: &str = "Notice a new invite or booking, so the user can check it for \
     conflicts and prepare.";

const EVENT_CANCELED_CONFIG: &[&str] = &["calendarId", "interval"];
const EVENT_CANCELED_USE: &str = "Tell the user a meeting was canceled and the time is free again.";

const ATTENDEE_RESPONSE_CONFIG: &[&str] = &["calendarId", "interval", "showDeleted"];
const ATTENDEE_RESPONSE_USE: &str =
    "Tell the user when someone accepts or declines a meeting they organized.";

const FILE_SHARED_CONFIG: &[&str] = &[
    "drive_id",
    "file_id",
    "interval",
    "page_size",
    "supports_all_drives",
];
const FILE_SHARED_USE: &str =
    "Tell the user a file was shared with them, and offer to summarize it.";

const COMMENT_ADDED_CONFIG: &[&str] = &["file_id", "interval", "max_files"];
const COMMENT_ADDED_USE: &str = "Surface comments on the user's documents that may need an answer.";

// ── gmail ───────────────────────────────────────────────────────────
/// The curated triggers of the `gmail` toolkit.
pub(super) const GMAIL_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "GMAIL_NEW_GMAIL_MESSAGE",
        toolkit: "gmail",
        event: TriggerEvent::NewEmail,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: NEW_MESSAGE_CONFIG,
        use_case: NEW_MESSAGE_USE,
    },
    CuratedTrigger {
        slug: "GMAIL_EMAIL_SENT_TRIGGER",
        toolkit: "gmail",
        event: TriggerEvent::EmailSent,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: EMAIL_SENT_CONFIG,
        use_case: EMAIL_SENT_USE,
    },
];

// ── googlecalendar ──────────────────────────────────────────────────
/// The curated triggers of the `googlecalendar` toolkit.
pub(super) const GOOGLECALENDAR_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "GOOGLECALENDAR_EVENT_STARTING_SOON_TRIGGER",
        toolkit: "googlecalendar",
        event: TriggerEvent::MeetingStartingSoon,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: STARTING_SOON_CONFIG,
        use_case: STARTING_SOON_USE,
    },
    CuratedTrigger {
        slug: "GOOGLECALENDAR_GOOGLE_CALENDAR_EVENT_CREATED_TRIGGER",
        toolkit: "googlecalendar",
        event: TriggerEvent::CalendarEventCreated,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: EVENT_CREATED_CONFIG,
        use_case: EVENT_CREATED_USE,
    },
    CuratedTrigger {
        slug: "GOOGLECALENDAR_EVENT_CANCELED_DELETED_TRIGGER",
        toolkit: "googlecalendar",
        event: TriggerEvent::CalendarEventCanceled,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: EVENT_CANCELED_CONFIG,
        use_case: EVENT_CANCELED_USE,
    },
    CuratedTrigger {
        slug: "GOOGLECALENDAR_ATTENDEE_RESPONSE_CHANGED_TRIGGER",
        toolkit: "googlecalendar",
        event: TriggerEvent::AttendeeResponseChanged,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: ATTENDEE_RESPONSE_CONFIG,
        use_case: ATTENDEE_RESPONSE_USE,
    },
];

// ── googledrive ─────────────────────────────────────────────────────
/// The curated triggers of the `googledrive` toolkit.
pub(super) const GOOGLEDRIVE_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "GOOGLEDRIVE_FILE_SHARED_PERMISSIONS_ADDED",
        toolkit: "googledrive",
        event: TriggerEvent::FileShared,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: FILE_SHARED_CONFIG,
        use_case: FILE_SHARED_USE,
    },
    CuratedTrigger {
        slug: "GOOGLEDRIVE_COMMENT_ADDED_TRIGGER",
        toolkit: "googledrive",
        event: TriggerEvent::DocumentComment,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: COMMENT_ADDED_CONFIG,
        use_case: COMMENT_ADDED_USE,
    },
];

// ── googlesuper ─────────────────────────────────────────────────────
/// The curated triggers of the `googlesuper` toolkit: one row per Gmail,
/// Calendar and Drive trigger above, configured the same way.
pub(super) const GOOGLESUPER_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "GOOGLESUPER_NEW_MESSAGE",
        toolkit: "googlesuper",
        event: TriggerEvent::NewEmail,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: NEW_MESSAGE_CONFIG,
        use_case: NEW_MESSAGE_USE,
    },
    CuratedTrigger {
        slug: "GOOGLESUPER_EMAIL_SENT_TRIGGER",
        toolkit: "googlesuper",
        event: TriggerEvent::EmailSent,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: EMAIL_SENT_CONFIG,
        use_case: EMAIL_SENT_USE,
    },
    CuratedTrigger {
        slug: "GOOGLESUPER_EVENT_STARTING_SOON_TRIGGER",
        toolkit: "googlesuper",
        event: TriggerEvent::MeetingStartingSoon,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: STARTING_SOON_CONFIG,
        use_case: STARTING_SOON_USE,
    },
    CuratedTrigger {
        slug: "GOOGLESUPER_GOOGLE_CALENDAR_EVENT_CREATED_TRIGGER",
        toolkit: "googlesuper",
        event: TriggerEvent::CalendarEventCreated,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: EVENT_CREATED_CONFIG,
        use_case: EVENT_CREATED_USE,
    },
    CuratedTrigger {
        slug: "GOOGLESUPER_EVENT_CANCELED_DELETED_TRIGGER",
        toolkit: "googlesuper",
        event: TriggerEvent::CalendarEventCanceled,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: EVENT_CANCELED_CONFIG,
        use_case: EVENT_CANCELED_USE,
    },
    CuratedTrigger {
        slug: "GOOGLESUPER_ATTENDEE_RESPONSE_CHANGED_TRIGGER",
        toolkit: "googlesuper",
        event: TriggerEvent::AttendeeResponseChanged,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: ATTENDEE_RESPONSE_CONFIG,
        use_case: ATTENDEE_RESPONSE_USE,
    },
    CuratedTrigger {
        slug: "GOOGLESUPER_FILE_SHARED_PERMISSIONS_ADDED",
        toolkit: "googlesuper",
        event: TriggerEvent::FileShared,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: FILE_SHARED_CONFIG,
        use_case: FILE_SHARED_USE,
    },
    CuratedTrigger {
        slug: "GOOGLESUPER_COMMENT_ADDED_TRIGGER",
        toolkit: "googlesuper",
        event: TriggerEvent::DocumentComment,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: COMMENT_ADDED_CONFIG,
        use_case: COMMENT_ADDED_USE,
    },
];
