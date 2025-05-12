use proton_core_api::services::proton::LabelId;
use stash::stash::Tether;

use crate::models::{MailSettings, Message, default_location::IncomingDefaultLocation};

use super::{MessageFlags, SystemLabelId};

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

    /// The system marked the message as spam.
    Spam,

    /// The message has an expiration date.
    Expiry {
        /// The Unix timestamp indicating when the message expires.
        timestamp: u64,
    },

    /// The message is scheduled for automatic deletion at a specific time because it is in spam or trash.
    AutoDelete {
        /// The Unix timestamp indicating when the message will be deleted.
        timestamp: u64,
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

impl Message {
    /// Get message banners.
    ///
    /// Fails if time went backwards
    pub async fn get_banners(&self, tether: &Tether) -> Vec<MessageBanner> {
        let mut banners = vec![];

        let flags = self.flags;

        let settings = &MailSettings::get_or_default(tether).await;

        let mut autodelete = false;

        // Banners that can only be displayed if the message is in the trash or spam folder:
        // Autodelete
        // Phishing OR SpamAuto
        if self
            .label_ids
            .iter()
            .any(|label| *label == LabelId::trash() || *label == LabelId::spam())
        {
            if let Some(days) = settings.auto_delete_spam_and_trash_days {
                // TODO: let chains
                if days != 0 && self.expiration_time != 0 {
                    banners.push(MessageBanner::AutoDelete {
                        timestamp: self.expiration_time,
                    });
                    autodelete = true;
                }
            }

            if flags.intersects(MessageFlags::PHISHING_AUTO | MessageFlags::PHISHING_MANUAL) {
                banners.push(MessageBanner::PhishingAttempt);
            } else if flags.intersects(MessageFlags::SPAM_AUTO) {
                banners.push(MessageBanner::Spam);
            } else {
                // manual spam don't get a banner
            }
        }

        // This check is here because we can't clear this on the local action
        if self.expiration_time != 0
            // Since the backend sends the expiration time for autodelete we have to do this to
            // disambiguate between autodelete and expiry and not show 2 banners.
            && !autodelete
        {
            banners.push(MessageBanner::Expiry {
                timestamp: self.expiration_time,
            });
        }

        if let Ok(Some(IncomingDefaultLocation::Blocked)) =
            IncomingDefaultLocation::find(self.sender.address.clone(), tether).await
        {
            banners.push(MessageBanner::BlockedSender);
        }

        banners.sort_unstable();
        banners
    }
}
