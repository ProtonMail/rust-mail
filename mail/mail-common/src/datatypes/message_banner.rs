use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::UnixTimestamp;
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
pub enum MessageBanner {
    /// The sender of this message is blocked.
    BlockedSender,

    /// The message might be a phishing attempt.
    PhishingAttempt {
        /// Whether the system or the user marked it as phishing.
        auto: bool,
    },

    /// The message has been marked as spam
    Spam {
        /// Whether the system or the user marked it as phishing.
        auto: bool,
    },

    /// The message has an expiration date.
    Expiry {
        /// The Unix timestamp indicating when the message expires.
        timestamp: UnixTimestamp,
    },

    /// The message is scheduled for automatic deletion at a specific time because it is in spam or trash.
    AutoDelete {
        /// The Unix timestamp indicating when the message will be deleted.
        timestamp: UnixTimestamp,
    },

    /// The message provides an option to unsubscribe from a newsletter.
    UnsubscribeNewsletter { already_unsubscribed: bool },

    /// The message is scheduled to be sent at a future time.
    ScheduledSend {
        /// The Unix timestamp indicating when the message is scheduled to be sent.
        timestamp: UnixTimestamp,
    },

    /// The message has been snoozed and will reappear later.
    Snoozed {
        /// The Unix timestamp indicating when the message will reappear.
        timestamp: UnixTimestamp,
    },

    /// The message contains embedded images.
    EmbeddedImages,

    /// The message contains remote content (e.g., external images or links).
    RemoteContent,

    /// The message could not be decrypted.
    UnableToDecrypt,
}

impl Message {
    pub async fn get_banners(&self, tether: &Tether) -> Vec<MessageBanner> {
        self.get_banners_inner(tether, false).await
    }

    pub async fn get_banners_inner(
        &self,
        tether: &Tether,
        can_unsubscribe: bool,
    ) -> Vec<MessageBanner> {
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
                if days != 0 && self.expiration_time != 0.into() {
                    banners.push(MessageBanner::AutoDelete {
                        timestamp: self.expiration_time,
                    });
                    autodelete = true;
                }
            }
        }

        if self.label_ids.contains(&LabelId::spam()) {
            if flags.intersects(
                MessageFlags::PHISHING_AUTO
                    | MessageFlags::PHISHING_MANUAL
                    | MessageFlags::FLAG_SUSPICIOUS,
            ) && !flags.intersects(MessageFlags::SPAM_MANUAL)
            {
                let auto = !flags.intersects(MessageFlags::PHISHING_MANUAL);
                banners.push(MessageBanner::PhishingAttempt { auto });
            } else {
                let auto = !flags.intersects(MessageFlags::SPAM_MANUAL);
                banners.push(MessageBanner::Spam { auto });
            }
        }

        // This check is here because we can't clear this on the local action
        if self.expiration_time != 0.into()
            // Since the backend sends the expiration time for autodelete we have to do this to
            // disambiguate between autodelete and expiry and not show 2 banners.
            && !autodelete
        {
            banners.push(MessageBanner::Expiry {
                timestamp: self.expiration_time,
            });
        }

        if let Ok(Some(IncomingDefaultLocation::Blocked)) = IncomingDefaultLocation::find(
            self.sender.address.clone().into_clear_text_string(),
            tether,
        )
        .await
        {
            banners.push(MessageBanner::BlockedSender);
        }

        if self.label_ids.contains(&LabelId::all_scheduled()) {
            banners.push(MessageBanner::ScheduledSend {
                timestamp: self.time,
            });
        }

        if self.label_ids.contains(&LabelId::snoozed()) {
            banners.push(MessageBanner::Snoozed {
                timestamp: self.snooze_time,
            });
        }

        if can_unsubscribe {
            let already_unsubscribed = self.flags.contains(MessageFlags::UNSUBSCRIBED);

            banners.push(MessageBanner::UnsubscribeNewsletter {
                already_unsubscribed,
            });
        }

        banners.sort_unstable();
        banners
    }
}
