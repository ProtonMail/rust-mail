mod attachments;

use crate::mail::{MailSessionError, MailUserSession};
use crate::new_live_query;
use stash::stash::StashError;
use std::sync::Arc;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailboxError {
    #[error("Could not find label with id '{0}'")]
    LabelNotFound(u64),
    #[error("Could not find label with remote id '{0}'")]
    RemoteLabelNotFound(LabelId),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(u64),
    #[error("{0}")]
    Context(
        #[from]
        #[source]
        MailSessionError,
    ),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::QueueError),
    #[error("Invalid Action: {0}")]
    InvalidAction(anyhow::Error),
    #[error("Conversation '{0}' not found")]
    ConversationNotFound(u64),
    #[error("Conversation '{0}' does not have a remote id")]
    ConversationDoesNotHaveRemoteId(u64),
    #[error("Problem with conversation with local ID: '{0}'")]
    ConversationError(u64),
    #[error("Could not find message with id '{0}'")]
    MessageNotFound(u64),
    #[error("Message '{0}' does not have a remote id")]
    MessageDoesNotHaveRemoteId(u64),
    #[error("Conversation '{0}' has no messages")]
    ConversationHasNoMessages(u64),
    #[error("API request failed with error: '{0}'")]
    APIError(RequestError),
    #[error("Mailbox is not in the right view mode for the current operation")]
    InvalidViewMode,
    #[error("Creating AddressDomainLogoDetails failed with error: '{0}'")]
    AddressDomainLogoError(AddressDomainLogoError),
    #[error("Attachment '{0}' not found")]
    AttachmentNotFound(u64),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(String),
    #[error("Message decryption error: {0}")]
    MessageDecryption(anyhow::Error),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Decrypted Message: {0}")]
    DecryptedMessage(#[from] proton_mail_common::DecryptedMessageError),
    #[error("{0}")]
    Other(anyhow::Error),
}

pub type MailboxResult<T> = Result<T, MailboxError>;

/// A [`Mailbox`] provides a gateway to manipulating messages and conversations for a given label.
#[derive(uniffi::Object)]
pub struct Mailbox {
    mbox: proton_mail_common::Mailbox,
}

/// Callback for operations that get scheduled in the background and return no result.
#[uniffi::export(callback_interface)]
pub trait MailboxBackgroundResult: Send + Sync {
    fn on_background_result(&self, error: Option<MailboxError>);
}

const DEFAULT_CONVERSATION_COUNT: usize = 50;

#[uniffi::export]
impl Mailbox {
    /// Create a new mailbox for a given label id.
    #[uniffi::constructor]
    pub async fn new(ctx: &MailUserSession, label_id: u64) -> MailboxResult<Self> {
        let mbox = proton_mail_common::Mailbox::with_id(ctx.ctx().clone(), label_id)?;
        if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
            error!("Could not sync mailbox: {e}");
        }
        Ok(Self { mbox })
    }

    /// Create a new mailbox for a given remote id.
    #[uniffi::constructor]
    pub async fn with_remote_id(ctx: &MailUserSession, label_id: &LabelId) -> MailboxResult<Self> {
        let mbox = proton_mail_common::Mailbox::with_remote_id(ctx.ctx().clone(), label_id).await?;
        Self::sync(mbox).await
    }

    /// Create a new mailbox for Inbox.
    #[uniffi::constructor]
    pub async fn inbox(ctx: &MailUserSession) -> MailboxResult<Self> {
        Self::with_remote_id(ctx, LabelId::inbox()).await
    }

    /// Create a new mailbox for a given label id.
    #[uniffi::constructor]
    pub async fn with_local_id(ctx: &MailUserSession, label_id: u64) -> MailboxResult<Self> {
        // Note: This is a workaround for the default constructor not being able to be
        // generated on Kotlin.
        Self::new(ctx, label_id).await
    }

    /// Get the label id of the mailbox.
    #[must_use]
    pub fn label_id(&self) -> u64 {
        self.mbox.label_id().value()
    }

    /// Get the mailbox's active view mode.
    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.mbox.view_mode()
    }
}

impl Mailbox {
    async fn sync(mbox: proton_mail_common::Mailbox) -> MailboxResult<Self> {
        if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
            error!("Could not sync mailbox: {e}");
        }
        Ok(Self { mbox })
    }
}

impl From<proton_mail_common::MailboxError> for MailboxError {
    fn from(value: proton_mail_common::MailboxError) -> Self {
        match value {
            proton_mail_common::MailboxError::LabelNotFound(e) => Self::LabelNotFound(e),
            proton_mail_common::MailboxError::RemoteLabelNotFound(e) => {
                Self::RemoteLabelNotFound(e)
            }
            proton_mail_common::MailboxError::LabelDoesNotHaveRemoteId(e) => {
                Self::LabelDoesNotHaveRemoteId(e)
            }
            proton_mail_common::MailboxError::Context(e) => Self::Context(e.into()),
            proton_mail_common::MailboxError::ActionQueue(e) => Self::ActionQueue(e),
            proton_mail_common::MailboxError::InvalidAction(e) => Self::InvalidAction(e),
            proton_mail_common::MailboxError::ConversationNotFound(e) => {
                Self::ConversationNotFound(e)
            }
            proton_mail_common::MailboxError::ConversationError(e) => Self::ConversationError(e),
            proton_mail_common::MailboxError::APIError(e) => Self::APIError(e),
            proton_mail_common::MailboxError::InvalidViewMode => Self::InvalidViewMode,
            proton_mail_common::MailboxError::AttachmentNotFound(e) => Self::AttachmentNotFound(e),
            proton_mail_common::MailboxError::AttachmentDecryption(e) => {
                Self::AttachmentDecryption(e.to_string())
            }
            proton_mail_common::MailboxError::AttachmentDecryptionIO(e) => {
                Self::AttachmentDecryption(e.to_string())
            }
            proton_mail_common::MailboxError::ConversationDoesNotHaveRemoteId(e) => {
                Self::ConversationDoesNotHaveRemoteId(e)
            }
            proton_mail_common::MailboxError::DB(e) => Self::DB(e),
            proton_mail_common::MailboxError::MessageDoesNotHaveRemoteId(e) => {
                Self::MessageDoesNotHaveRemoteId(e)
            }
            proton_mail_common::MailboxError::MessageDecryption(e) => {
                Self::MessageDecryption(anyhow!("{e}"))
            }
            proton_mail_common::MailboxError::ConversationHasNoMessages(e) => {
                Self::ConversationHasNoMessages(e)
            }
            proton_mail_common::MailboxError::DecryptedMessage(e) => Self::DecryptedMessage(e),
        }
    }
}
