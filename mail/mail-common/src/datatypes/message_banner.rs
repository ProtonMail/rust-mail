/// Represents different types of banners that can be displayed for a given message.
/// These banners indicate various security warnings, expiration notices,
/// or content-related alerts.
///
/// The order of the variants is important as they are sorted.
/// Check with product before adding new banners to check the order of the new banner.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
