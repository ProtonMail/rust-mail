pub mod attachments;
pub mod decrypted_message;

use crate::datatypes::{MessageRecipientDisplayMode, ViewMode};
use crate::models::{ConversationCounters, MailLabel, MessageCounters};
use crate::{AppError, MailContextResult};
pub use attachments::DecryptedAttachment;
use parking_lot::RwLock;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelExtension as _, ModelIdExtension as _};
use stash::orm::Model;
use stash::stash::{Stash, Tether, WatcherHandle};
use std::sync::Arc;
use tracing::{debug, instrument};

/// Represents an open label through which one can access the messages or conversations.
///
/// Mailboxes can either be in conversation or message view mode depending on the value
/// of the user's [`MailUserSettings`] values or if the label has special rules related
/// to how it should be presented.
///
/// Before creating a live query, check the value of [`Mailbox::view_mode()`] to verify
/// which is the correct mode.
#[derive(Clone, Debug)]
pub struct Mailbox {
    state: Arc<RwLock<MailboxState>>,
}

impl Mailbox {
    pub async fn new(tether: &Tether, label_id: LocalLabelId) -> MailContextResult<Self> {
        let label = Label::load(label_id, tether)
            .await?
            .ok_or(AppError::LabelNotFound(label_id))?;

        let view_mode = label.view_mode(tether).await?;

        debug!("Creating Mailbox ({}, view_mode={:?})", label_id, view_mode);

        let state = MailboxState {
            label_id,
            view_mode,
            recipient_display_mode: label.recipient_display_mode(),
        };

        Ok(Self {
            state: Arc::new(RwLock::new(state)),
        })
    }

    pub async fn with_remote_id(tether: &Tether, label_id: LabelId) -> MailContextResult<Self> {
        let label = Label::find_by_remote_id(label_id, tether)
            .await?
            .expect("Label not found");

        let label_id = label.id();
        let view_mode = label.view_mode(tether).await?;

        debug!("Creating Mailbox ({}, view_mode={:?})", label_id, view_mode);

        let state = MailboxState {
            label_id,
            view_mode,
            recipient_display_mode: label.recipient_display_mode(),
        };

        Ok(Self {
            state: Arc::new(RwLock::new(state)),
        })
    }

    #[instrument(skip(self, tether))]
    pub async fn change_label(
        &self,
        tether: &Tether,
        label_id: LocalLabelId,
    ) -> MailContextResult<()> {
        debug!("Updating Mailbox");

        let label = Label::load(label_id, tether)
            .await?
            .ok_or(AppError::LabelNotFound(label_id))?;

        let view_mode = label.view_mode(tether).await?;
        let recipient_display_mode = label.recipient_display_mode();

        // ---

        let mut state = self.state.write();

        state.label_id = label_id;
        state.view_mode = view_mode;
        state.recipient_display_mode = recipient_display_mode;

        Ok(())
    }

    pub fn label_id(&self) -> LocalLabelId {
        self.state.read().label_id
    }

    pub fn view_mode(&self) -> ViewMode {
        self.state.read().view_mode
    }

    pub fn recipient_display_mode(&self) -> MessageRecipientDisplayMode {
        self.state.read().recipient_display_mode
    }

    /// Get the number of unread items in this mailbox.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn unread_count(&self, tether: &Tether) -> MailContextResult<u64> {
        Ok(match self.view_mode() {
            ViewMode::Conversations => {
                let counters = ConversationCounters::find_by_id(self.label_id(), tether).await?;
                counters.map(|c| c.unread).unwrap_or_default()
            }
            ViewMode::Messages => {
                let counters = MessageCounters::find_by_id(self.label_id(), tether).await?;
                counters.map(|c| c.unread).unwrap_or_default()
            }
        })
    }

    /// Subscribe for updates to the number of unread items in this mailbox.
    /// Depending on the view mode it either watches conversations or messages.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    ///
    pub async fn watch_unread_count(&self, stash: &Stash) -> MailContextResult<WatcherHandle> {
        let watcher = match self.view_mode() {
            ViewMode::Conversations => ConversationCounters::watch(stash).await?,
            ViewMode::Messages => MessageCounters::watch(stash).await?,
        };

        Ok(watcher)
    }
}

#[derive(Clone, Debug)]
struct MailboxState {
    label_id: LocalLabelId,
    view_mode: ViewMode,
    recipient_display_mode: MessageRecipientDisplayMode,
}

#[cfg(any(feature = "test-utils", test))]
mod test_utils {
    use super::*;
    use crate::MailContextError;
    use crate::models::{Conversation, MailboxLabels, Message};
    use futures::TryFutureExt;
    use proton_core_api::session::Session;
    use tracing::error;

    impl Mailbox {
        /// Sync the label's messages or conversations.
        ///
        /// Depending on the user's mail settings, this function will either sync the conversations
        /// or the messages of the label.
        ///
        /// # Errors
        /// Returns error if API request or database changes failed.
        #[tracing::instrument(skip_all)]
        pub async fn sync(
            &self,
            tether: &mut Tether,
            api: &Session,
            count: usize,
        ) -> MailContextResult<()> {
            let Some(label) = Label::load(self.label_id(), tether).await? else {
                return Err(AppError::LabelNotFound(self.label_id()).into());
            };

            let Some(remote_id) = label.remote_id.clone() else {
                return Err(AppError::LabelDoesNotHaveRemoteId(self.label_id()).into());
            };

            debug!("Syncing {}({})", self.label_id(), &remote_id);

            let mut mailbox_label = MailboxLabels::find_by_id(self.label_id(), tether)
                .await?
                .unwrap_or_else(|| MailboxLabels::new(self.label_id()));
            if mailbox_label.initialized {
                debug!("Label {} already initialized, skipping", self.label_id());
                return Ok(());
            }
            debug!(
                "Label {} not initialized, fetching (mode={:?})",
                self.label_id(),
                self.view_mode()
            );

            match self.view_mode() {
                ViewMode::Conversations => {
                    Conversation::sync_first_conversation_page(remote_id, count, api, tether)
                        .inspect_err(|e| error!("Failed to sync conversations for label: {e:?}"))
                        .await
                }

                ViewMode::Messages => {
                    Message::sync_first_message_page(remote_id, count, api, tether)
                        .inspect_err(|e| error!("Failed to sync messages for label: {e:?}"))
                        .await
                }
            }?;

            mailbox_label.initialized = true;
            tether
                .tx(async |tx| {
                    mailbox_label.save(tx).await.map_err(|e| {
                        error!("Failed to mark label as initialized: {e:?}");
                        MailContextError::Stash(e)
                    })
                })
                .await?;

            debug!("Syncing finished");
            Ok(())
        }
    }
}
