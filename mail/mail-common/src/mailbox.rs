pub mod attachments;

pub mod decrypted_message;

use crate::datatypes::{LocalAttachmentId, ViewMode};
use crate::models::{
    Conversation, ConversationCounters, MailLabel, MailboxLabels, Message, MessageCounters,
};
use crate::{AppError, MailContextError};
pub use attachments::DecryptedAttachment;
use futures::TryFutureExt;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::LabelId;
use proton_api_core::services::proton::Proton;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelExtension as _, ModelIdExtension as _};
use proton_crypto_inbox::attachment::AttachmentDecryptionError;
use stash::orm::Model;
use stash::stash::{Stash, StashError, Tether, WatcherHandle};
use tracing::{debug, error};

#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    #[error("Could not find label with local id '{0}'")]
    LabelNotFound(LocalLabelId),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(LocalLabelId),
    #[error("Attachment '{0}' not found")]
    AttachmentNotFound(LocalAttachmentId),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(#[from] AttachmentDecryptionError),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryptionIO(String),
    #[error("Attachment '{0}' does not have a remote id")]
    AttachmentDoesNotHaveRemoteId(LocalAttachmentId),
    #[error("App error: {0}")]
    AppError(#[from] AppError),
    #[error("API request failed with error: '{0}'")]
    APIError(#[from] ApiServiceError),
    #[error("{0}")]
    Context(
        #[from]
        #[source]
        MailContextError,
    ),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::queue::Error),
    #[error("Action is not valid: {0}")]
    InvalidAction(anyhow::Error),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Message decryption error: {0}")]
    MessageDecryption(#[from] proton_crypto_inbox::message::MessageError),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
}

pub type MailboxResult<T> = Result<T, MailboxError>;

/// Represents an open label through which one can access the messages or conversations.
///
/// Mailboxes can either be in conversation or message view mode depending on the value
/// of the user's [`MailUserSettings`] values or if the label has special rules related
/// to how it should be presented.
///
/// Before creating a live query, check the value of [`Mailbox::view_mode()`] to verify
/// which is the correct mode.
#[derive(Clone)]
pub struct Mailbox {
    label_id: LocalLabelId,
    view_mode: ViewMode,
}

impl Mailbox {
    pub async fn new(tether: &Tether, label_id: LocalLabelId) -> MailboxResult<Self> {
        let label = Label::load(label_id, tether)
            .await?
            .ok_or(MailboxError::LabelNotFound(label_id))?;

        let view_mode = label.view_mode(tether).await?;
        debug!("Creating Mailbox ({}, view_mode={:?})", label_id, view_mode);

        Ok(Self {
            label_id,
            view_mode,
        })
    }

    pub async fn with_remote_id(tether: &Tether, label_id: LabelId) -> MailboxResult<Self> {
        let label = Label::find_by_remote_id(label_id, tether)
            .await?
            .expect("Label not found");

        let label_id = label.local_id.expect("Label has no local id");
        let view_mode = label.view_mode(tether).await?;
        debug!("Creating Mailbox ({}, view_mode={:?})", label_id, view_mode);

        Ok(Self {
            label_id,
            view_mode,
        })
    }

    pub fn label_id(&self) -> LocalLabelId {
        self.label_id
    }

    /// Sync the label's messages or conversations.
    ///
    /// Depending on the user's mail settings, this function will either sync the conversations
    /// or the messages of the label.
    ///
    /// # Errors
    /// Returns error if API request or database changes failed.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip_all)]
    pub async fn sync(&self, tether: &mut Tether, api: &Proton, count: usize) -> MailboxResult<()> {
        let Some(label) = Label::load(self.label_id, tether).await? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };

        let Some(remote_id) = label.remote_id.clone() else {
            return Err(MailboxError::LabelDoesNotHaveRemoteId(self.label_id));
        };

        debug!("Syncing {}({})", self.label_id, &remote_id);

        let mut mailbox_label = MailboxLabels::find_by_id(self.label_id, tether)
            .await?
            .unwrap_or_else(|| MailboxLabels::new(self.label_id));
        if mailbox_label.initialized {
            debug!("Label {} already initialized, skipping", self.label_id);
            return Ok(());
        }
        debug!(
            "Label {} not initialized, fetching (mode={:?})",
            self.label_id, self.view_mode
        );

        match self.view_mode {
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

    /// The mailbox's current view mode.
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Get the number of unread items in this mailbox.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn unread_count(&self, tether: &Tether) -> Result<u64, MailboxError> {
        Ok(match self.view_mode {
            ViewMode::Conversations => {
                let counters = ConversationCounters::find_by_id(self.label_id, tether).await?;
                counters.map(|c| c.unread).unwrap_or_default()
            }
            ViewMode::Messages => {
                let counters = MessageCounters::find_by_id(self.label_id, tether).await?;
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
    pub async fn watch_unread_count(&self, stash: &Stash) -> Result<WatcherHandle, MailboxError> {
        let watcher = match self.view_mode {
            ViewMode::Conversations => ConversationCounters::watch(stash)?,
            ViewMode::Messages => MessageCounters::watch(stash)?,
        };

        Ok(watcher)
    }
}
