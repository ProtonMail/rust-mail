mod attachments;
mod conversations;
mod labels;
mod messages;

use crate::mail::{MailSessionError, MailUserSession};
use crate::new_live_query;
use proton_mail_common::db::proton_sqlite3::SharedLive;
use proton_mail_common::db::{
    ConversationQuery, LocalAttachmentId, LocalConversationId, LocalLabelId, LocalMessageId,
};
use proton_mail_common::exports::anyhow::anyhow;
use proton_mail_common::exports::proton_sqlite3::{
    InProcessTrackerService, LiveQueryUpdated, Observable, SharedLiveQueryBuilder,
};
use proton_mail_common::exports::tracing::error;
use proton_mail_common::exports::{anyhow, thiserror};
use proton_mail_common::proton_api_mail::domain::{
    AddressDomainLogoError, LabelId, MailSettingsViewMode,
};
use proton_mail_common::proton_api_mail::proton_api_core::http::RequestError;
use proton_mail_common::MailboxObservableQueryBuilder;
use std::future::Future;
use std::sync::Arc;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailboxError {
    #[error("Could not find label with id '{0}'")]
    LabelNotFound(LocalLabelId),
    #[error("Could not find label with remote id '{0}'")]
    RemoteLabelNotFound(LabelId),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(LocalLabelId),
    #[error("{0}")]
    Context(
        #[from]
        #[source]
        MailSessionError,
    ),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_mail_common::exports::proton_action_queue::QueueError),
    #[error("Invalid Action: {0}")]
    InvalidAction(anyhow::Error),
    #[error("Conversation '{0}' not found")]
    ConversationNotFound(LocalConversationId),
    #[error("Conversation '{0}' does not have a remote id")]
    ConversationDoesNotHaveRemoteId(LocalConversationId),
    #[error("Problem with conversation with local ID: '{0}'")]
    ConversationError(LocalConversationId),
    #[error("Message '{0}' does not have a remote id")]
    MessageDoesNotHaveRemoteId(LocalMessageId),
    #[error("API request failed with error: '{0}'")]
    APIError(RequestError),
    #[error("Mailbox is not in the right view mode for the current operation")]
    InvalidViewMode,
    #[error("Creating AddressDomainLogoDetails failed with error: '{0}'")]
    AddressDomainLogoError(AddressDomainLogoError),
    #[error("Attachment '{0}' not found")]
    AttachmentNotFound(LocalAttachmentId),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(String),
    #[error("Database Error: {0}")]
    DB(#[from] proton_mail_common::db::DBError),
    #[error("Message decryption error: {0}")]
    MessageDecryption(anyhow::Error),
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

/// Callback for a labels view data change.
#[uniffi::export(callback_interface)]
pub trait MailboxLiveQueryUpdatedCallback: Send + Sync {
    fn on_updated(&self);
}

impl LiveQueryUpdated for Box<dyn MailboxLiveQueryUpdatedCallback> {
    fn on_live_query_updated(&self) {
        self.on_updated();
    }
}

new_live_query!(MailboxConversationLiveQuery, ConversationQuery);

const DEFAULT_CONVERSATION_COUNT: usize = 50;

#[uniffi::export]
impl Mailbox {
    /// Create a new mailbox for a given label id.
    #[uniffi::constructor]
    pub async fn new(ctx: &MailUserSession, label_id: u64) -> MailboxResult<Self> {
        let mbox =
            proton_mail_common::Mailbox::with_id(ctx.ctx().clone(), LocalLabelId::new(label_id))?;
        Self::sync(mbox).await
    }

    /// Create a new mailbox for a given remote id.
    #[uniffi::constructor]
    pub async fn with_remote_id(ctx: &MailUserSession, label_id: &LabelId) -> MailboxResult<Self> {
        let mbox = proton_mail_common::Mailbox::with_remote_id(ctx.ctx().clone(), label_id)?;
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
    pub fn view_mode(&self) -> MailSettingsViewMode {
        self.mbox.view_mode()
    }
}

impl Mailbox {
    async fn sync(mbox: proton_mail_common::Mailbox) -> MailboxResult<Self> {
        let uniffi_mbox = Self { mbox: mbox.clone() };

        uniffi_mbox
            .uniffi_async(async move {
                if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
                    error!("Could not sync mailbox: {e}");
                }
                Ok(())
            })
            .await?;

        Ok(uniffi_mbox)
    }

    /// Helper function to hide implementation details of how to run async code with
    /// uniffi.
    pub(crate) async fn uniffi_async<T, F>(&self, f: F) -> Result<T, MailboxError>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, MailboxError>> + Send + 'static,
    {
        self.mbox
            .user_context()
            .mail_context()
            .async_runtime()
            .spawn(f)
            .await
            .map_err(|err| MailboxError::Other(anyhow!("Failed to join task: {err}")))?
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
        }
    }
}

/*
struct FFIMailboxBackgroundVoidResult(Box<dyn MailboxBackgroundResult>);

impl FFIMailboxBackgroundVoidResult {
    fn boxed(self) -> Box<dyn proton_mail_common::MailboxBackgroundResult<()>> {
        Box::new(self)
    }
}

impl From<Box<dyn MailboxBackgroundResult>> for FFIMailboxBackgroundVoidResult {
    fn from(value: Box<dyn MailboxBackgroundResult>) -> Self {
        Self(value)
    }
}

impl proton_mail_common::MailboxBackgroundResult<()> for FFIMailboxBackgroundVoidResult {
    fn on_background_result(&self, result: proton_mail_common::MailboxResult<()>) {
        let result = if let Err(e) = result {
            Some(e.into())
        } else {
            None
        };
        self.0.on_background_result(result);
    }
}*/

struct FFIObservableConversationsQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<ConversationQuery> for FFIObservableConversationsQueryBuilder {
    type Output = Arc<MailboxConversationLiveQuery>;

    fn build(self, tracker: InProcessTrackerService, query: ConversationQuery) -> Self::Output {
        MailboxConversationLiveQuery::new(tracker, query, self.0)
    }
}
