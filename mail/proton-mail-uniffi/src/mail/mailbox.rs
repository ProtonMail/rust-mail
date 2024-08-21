mod attachments;

use crate::core::datatypes::Id;
use crate::mail::datatypes::ViewMode;
use crate::mail::{MailSessionError, MailUserSession};
use crate::uniffi_async;
use anyhow::anyhow;
use proton_action_queue::queue::Error as QueueError;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::Proton;
use proton_core_common::cache::CacheError;
use proton_core_common::datatypes::LabelId as RealLabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::{AppError, MailboxError as RealMailboxError};
use stash::stash::{Stash, StashError};
use std::sync::Arc;
use tokio::task::JoinError;
use tracing::error;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailboxError {
    #[error("Could not find label with id '{0}'")]
    LabelNotFound(Id),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(Id),
    #[error("No exclusive location found for message '{0}'")]
    NoExclusiveLocationFound(Id),
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
    ConversationNotFound(Id),
    #[error("Conversation '{0}' does not have a remote id")]
    ConversationDoesNotHaveRemoteId(Id),
    #[error("Problem with conversation with local ID: '{0}'")]
    ConversationError(Id),
    #[error("Could not find message with id '{0}'")]
    MessageNotFound(Id),
    #[error("Message '{0}' does not have a remote id")]
    MessageDoesNotHaveRemoteId(Id),
    #[error("Conversation '{0}' has no messages")]
    ConversationHasNoMessages(Id),
    #[error("App error: {0}")]
    AppError(#[from] AppError),
    #[error("API request failed with error: '{0}'")]
    APIError(ApiServiceError),
    #[error("Mailbox is not in the right view mode for the current operation")]
    InvalidViewMode,
    #[error("Attachment '{0}' not found")]
    AttachmentNotFound(Id),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(String),
    #[error("Attachment '{0}' does not have a remote id")]
    AttachmentDoesNotHaveRemoteId(Id),
    #[error("Message decryption error: {0}")]
    MessageDecryption(anyhow::Error),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("{0}")]
    Other(anyhow::Error),
}

impl From<JoinError> for MailboxError {
    fn from(value: JoinError) -> Self {
        Self::Other(anyhow::Error::new(value))
    }
}

pub type MailboxResult<T> = Result<T, MailboxError>;

/// A [`Mailbox`] provides a gateway to manipulating messages and conversations for a given label.
#[derive(uniffi::Object)]
pub struct Mailbox {
    /// The inner mailbox, which is the real internal type.
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
    pub async fn new(ctx: &MailUserSession, label_id: Id) -> MailboxResult<Arc<Self>> {
        let ctx = ctx.ctx().clone();
        uniffi_async(async move {
            let mbox = proton_mail_common::Mailbox::new(ctx, label_id.into()).await?;
            if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
                error!("Could not sync mailbox: {e}");
            }
            Ok(Arc::new(Self { mbox }))
        })
        .await
    }

    /// Create a new mailbox for Inbox.
    #[uniffi::constructor]
    pub async fn inbox(ctx: &MailUserSession) -> MailboxResult<Arc<Self>> {
        let mbox =
            proton_mail_common::Mailbox::with_remote_id(ctx.ctx().clone(), RealLabelId::inbox())
                .await?;
        Self::sync(mbox).await
    }

    /// Create a new mailbox for a given label id.
    #[uniffi::constructor]
    pub async fn with_label_id(ctx: &MailUserSession, label_id: Id) -> MailboxResult<Arc<Self>> {
        // Note: This is a workaround for the default constructor not being able to be
        // generated on Kotlin.
        Self::new(ctx, label_id).await
    }

    /// Get the label id of the mailbox.
    #[must_use]
    pub fn label_id(&self) -> Id {
        self.mbox.label_id().into()
    }

    /// Get the mailbox's active view mode.
    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.mbox.view_mode().into()
    }
}

impl Mailbox {
    /// Get the inner mailbox.
    #[must_use]
    pub fn mbox(&self) -> &proton_mail_common::Mailbox {
        &self.mbox
    }

    /// Get the API service.
    #[must_use]
    pub fn api(&self) -> &Proton {
        self.mbox.api()
    }

    /// Get the database connection.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        self.mbox.stash()
    }

    async fn sync(mbox: proton_mail_common::Mailbox) -> MailboxResult<Arc<Self>> {
        if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
            error!("Could not sync mailbox: {e}");
        }
        Ok(Arc::new(Self { mbox }))
    }
}

impl From<RealMailboxError> for MailboxError {
    fn from(value: RealMailboxError) -> Self {
        match value {
            RealMailboxError::LabelNotFound(e) => Self::LabelNotFound(e.into()),
            RealMailboxError::LabelDoesNotHaveRemoteId(e) => {
                Self::LabelDoesNotHaveRemoteId(e.into())
            }
            RealMailboxError::Context(e) => Self::Context(e.into()),
            RealMailboxError::ActionQueue(e) => Self::ActionQueue(e),
            RealMailboxError::InvalidAction(e) => Self::InvalidAction(e),
            RealMailboxError::ConversationNotFound(e) => Self::ConversationNotFound(e.into()),
            RealMailboxError::ConversationError(e) => Self::ConversationError(e.into()),
            RealMailboxError::APIError(e) => Self::APIError(e),
            RealMailboxError::InvalidViewMode => Self::InvalidViewMode,
            RealMailboxError::AttachmentNotFound(e) => Self::AttachmentNotFound(e.into()),
            RealMailboxError::AttachmentDecryption(e) => Self::AttachmentDecryption(e.to_string()),
            RealMailboxError::AttachmentDecryptionIO(e) => {
                Self::AttachmentDecryption(e.to_string())
            }
            RealMailboxError::ConversationDoesNotHaveRemoteId(e) => {
                Self::ConversationDoesNotHaveRemoteId(e.into())
            }
            RealMailboxError::Stash(e) => Self::Stash(e),
            RealMailboxError::MessageDoesNotHaveRemoteId(e) => {
                Self::MessageDoesNotHaveRemoteId(e.into())
            }
            RealMailboxError::MessageDecryption(e) => Self::MessageDecryption(anyhow!("{e}")),
            RealMailboxError::ConversationHasNoMessages(e) => {
                Self::ConversationHasNoMessages(e.into())
            }
            RealMailboxError::AttachmentDoesNotHaveRemoteId(e) => {
                Self::AttachmentDoesNotHaveRemoteId(e.into())
            }
            RealMailboxError::MessageNotFound(e) => Self::MessageNotFound(e.into()),
            RealMailboxError::AppError(e) => Self::AppError(e),
            RealMailboxError::NoExclusiveLocationFound(e) => {
                Self::NoExclusiveLocationFound(e.into())
            }
            RealMailboxError::Cache(e) => Self::Cache(e),
            RealMailboxError::IO(e) => Self::IO(e),
        }
    }
}
