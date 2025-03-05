use proton_mail_common::datatypes::message_banner::MessageBanner as RealMessageBanner;

/// Represents different types of banners that can be displayed for a given message.
/// These banners indicate various security warnings, expiration notices,
/// or content-related alerts.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, uniffi::Enum)]
pub enum MessageBanner {
    /// The sender of this message is blocked.
    BlockedSender,

    /// The message might be a phishing attempt.
    PhishingAttempt,

    /// The message has been marked as spam.
    Spam,

    /// The message has an expiration date.
    Expiry {
        /// The Unix timestamp indicating when the message expires.
        timestamp: u64,
    },

    /// The message is scheduled for automatic deletion at a specific time.
    AutoDelete {
        /// The Unix timestamp indicating when the message will be deleted.
        timestamp: u64,

        /// How many days a message will stay in trash/spam until it expires
        // FIXME: Delete this field in favor of timestamp
        delete_days: u32,
    },

    /// The message provides an option to unsubscribe from a newsletter.
    UnsubscribeNewsletter,

    /// The message is scheduled to be sent at a future time.
    ScheduledSend {
        /// The Unix timestamp indicating when the message is scheduled to be sent.
        timestamp: u64,
    },

    /// The message has been snoozed and will reappear later.
    Snoozed {
        /// The Unix timestamp indicating when the message will reappear.
        timestamp: u64,
    },

    /// The message contains embedded images.
    EmbeddedImages,

    /// The message contains remote content (e.g., external images or links).
    RemoteContent,
}

impl From<RealMessageBanner> for MessageBanner {
    fn from(value: RealMessageBanner) -> Self {
        match value {
            RealMessageBanner::BlockedSender => Self::BlockedSender,
            RealMessageBanner::PhishingAttempt => Self::PhishingAttempt,
            RealMessageBanner::Spam => Self::Spam,
            RealMessageBanner::Expiry { timestamp } => Self::Expiry { timestamp },
            RealMessageBanner::AutoDelete {
                timestamp,
                delete_days,
            } => Self::AutoDelete {
                timestamp,
                delete_days,
            },
            RealMessageBanner::UnsubscribeNewsletter => Self::UnsubscribeNewsletter,
            RealMessageBanner::ScheduledSend { timestamp } => Self::ScheduledSend { timestamp },
            RealMessageBanner::Snoozed { timestamp } => Self::Snoozed { timestamp },
            RealMessageBanner::EmbeddedImages => Self::EmbeddedImages,
            RealMessageBanner::RemoteContent => Self::RemoteContent,
        }
    }
}
