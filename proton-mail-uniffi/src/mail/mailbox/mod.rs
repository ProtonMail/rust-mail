mod attachments;

use crate::core::datatypes::LabelId;
use crate::mail::datatypes::ViewMode;
use crate::mail::{MailSessionError, MailUserSession};
use anyhow::anyhow;
use proton_action_queue::queue::Error as QueueError;
use proton_api_core::service::ApiServiceError;
use proton_core_common::datatypes::LabelId as RealLabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::decrypted_message::DecryptedMessageError;
use proton_mail_common::{AppError, MailboxError as RealMailboxError};
use stash::stash::StashError;
use tracing::error;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailboxError {
    #[error("Could not find label with id '{0}'")]
    LabelNotFound(u64),
    #[error("Could not find label with remote id '{0}'")]
    RemoteLabelNotFound(LabelId),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(u64),
    #[error("No exclusive location found for message '{0}'")]
    NoExclusiveLocationFound(u64),
    #[error("{0}")]
    Context(
        #[from]
        #[source]
        MailSessionError,
    ),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] QueueError),
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
    #[error("App error: {0}")]
    AppError(#[from] AppError),
    #[error("API request failed with error: '{0}'")]
    APIError(ApiServiceError),
    #[error("Mailbox is not in the right view mode for the current operation")]
    InvalidViewMode,
    #[error("Attachment '{0}' not found")]
    AttachmentNotFound(u64),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(String),
    #[error("Attachment '{0}' does not have a remote id")]
    AttachmentDoesNotHaveRemoteId(u64),
    #[error("Message decryption error: {0}")]
    MessageDecryption(anyhow::Error),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Decrypted Message: {0}")]
    DecryptedMessage(#[from] DecryptedMessageError),
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
        let mbox = proton_mail_common::Mailbox::new(ctx.ctx().clone(), label_id).await?;
        if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
            error!("Could not sync mailbox: {e}");
        }
        Ok(Self { mbox })
    }

    /// Create a new mailbox for a given remote id.
    #[uniffi::constructor]
    pub async fn with_remote_id(ctx: &MailUserSession, label_id: &LabelId) -> MailboxResult<Self> {
        let mbox =
            proton_mail_common::Mailbox::with_remote_id(ctx.ctx().clone(), label_id.clone().into())
                .await?;
        Self::sync(mbox).await
    }

    /// Create a new mailbox for Inbox.
    #[uniffi::constructor]
    pub async fn inbox(ctx: &MailUserSession) -> MailboxResult<Self> {
        Self::with_remote_id(ctx, &RealLabelId::inbox().into()).await
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
        self.mbox.label_id()
    }

    /// Get the mailbox's active view mode.
    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.mbox.view_mode().into()
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

impl From<RealMailboxError> for MailboxError {
    fn from(value: RealMailboxError) -> Self {
        match value {
            RealMailboxError::LabelNotFound(e) => Self::LabelNotFound(e),
            RealMailboxError::RemoteLabelNotFound(e) => Self::RemoteLabelNotFound(e.into()),
            RealMailboxError::LabelDoesNotHaveRemoteId(e) => Self::LabelDoesNotHaveRemoteId(e),
            RealMailboxError::Context(e) => Self::Context(e.into()),
            RealMailboxError::ActionQueue(e) => Self::ActionQueue(e),
            RealMailboxError::InvalidAction(e) => Self::InvalidAction(e),
            RealMailboxError::ConversationNotFound(e) => Self::ConversationNotFound(e),
            RealMailboxError::ConversationError(e) => Self::ConversationError(e),
            RealMailboxError::APIError(e) => Self::APIError(e),
            RealMailboxError::InvalidViewMode => Self::InvalidViewMode,
            RealMailboxError::AttachmentNotFound(e) => Self::AttachmentNotFound(e),
            RealMailboxError::AttachmentDecryption(e) => Self::AttachmentDecryption(e.to_string()),
            RealMailboxError::AttachmentDecryptionIO(e) => {
                Self::AttachmentDecryption(e.to_string())
            }
            RealMailboxError::ConversationDoesNotHaveRemoteId(e) => {
                Self::ConversationDoesNotHaveRemoteId(e)
            }
            RealMailboxError::Stash(e) => Self::Stash(e),
            RealMailboxError::MessageDoesNotHaveRemoteId(e) => Self::MessageDoesNotHaveRemoteId(e),
            RealMailboxError::MessageDecryption(e) => Self::MessageDecryption(anyhow!("{e}")),
            RealMailboxError::ConversationHasNoMessages(e) => Self::ConversationHasNoMessages(e),
            RealMailboxError::DecryptedMessage(e) => Self::DecryptedMessage(e),
            RealMailboxError::AttachmentDoesNotHaveRemoteId(e) => {
                Self::AttachmentDoesNotHaveRemoteId(e)
            }
            RealMailboxError::MessageNotFound(e) => Self::MessageNotFound(e),
            RealMailboxError::AppError(e) => Self::AppError(e),
            RealMailboxError::NoExclusiveLocationFound(e) => Self::NoExclusiveLocationFound(e),
        }
    }
}
