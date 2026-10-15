//! Curated triggers — Microsoft toolkits: Outlook mail and calendar.
//!
//! Outlook delivers by webhook, so none of these takes an `interval`.

use super::types::{CuratedTrigger, TriggerDelivery, TriggerEvent, TriggerTier};

/// The filters Outlook's received- and sent-mail triggers both accept.
const MESSAGE_FILTER_CONFIG: &[&str] = &[
    "folder_id",
    "from_email",
    "has_attachments",
    "importance",
    "subject_contains",
    "to_email",
];

// ── outlook ─────────────────────────────────────────────────────────
/// The curated triggers of the `outlook` toolkit.
pub(super) const OUTLOOK_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "OUTLOOK_MESSAGE_TRIGGER",
        toolkit: "outlook",
        event: TriggerEvent::NewEmail,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: MESSAGE_FILTER_CONFIG,
        use_case: "Alert the user to important mail, or to mail from people they care about.",
    },
    CuratedTrigger {
        slug: "OUTLOOK_EVENT_TRIGGER",
        toolkit: "outlook",
        event: TriggerEvent::CalendarEventCreated,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: &[],
        use_case: "Notice a new invite or booking, so the user can check it for conflicts and \
                   prepare.",
    },
    CuratedTrigger {
        slug: "OUTLOOK_SENT_MESSAGE_TRIGGER",
        toolkit: "outlook",
        event: TriggerEvent::EmailSent,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: MESSAGE_FILTER_CONFIG,
        use_case: "Remind the user to follow up when a mail they sent gets no reply.",
    },
];
