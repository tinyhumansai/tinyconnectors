//! Curated triggers — messaging toolkits: Slack.

use super::types::{CuratedTrigger, TriggerDelivery, TriggerEvent, TriggerTier};

// ── slack ───────────────────────────────────────────────────────────
/// The curated triggers of the `slack` toolkit.
pub(super) const SLACK_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "SLACK_DIRECT_MESSAGE_RECEIVED",
        toolkit: "slack",
        event: TriggerEvent::DirectMessage,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::Core,
        required_config: &[],
        optional_config: &["is_thread_reply"],
        use_case: "Surface direct messages while the user is away from Slack.",
    },
    CuratedTrigger {
        slug: "SLACK_CHANNEL_MESSAGE_RECEIVED",
        toolkit: "slack",
        event: TriggerEvent::ChannelMessage,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: &[
            "channel_id",
            "channel_type",
            "is_bot_message",
            "is_thread_reply",
        ],
        use_case: "Watch a channel for mentions of the user or keywords they care about.",
    },
    CuratedTrigger {
        slug: "SLACK_REACTION_ADDED",
        toolkit: "slack",
        event: TriggerEvent::ReactionAdded,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: &[],
        use_case: "Treat a chosen reaction as a request, such as turning a message the user \
                   marked into a task or reminder.",
    },
];
