mod attachments;
mod conversation;
mod messages;

#[cfg(test)]
mod tests;

pub use attachments::DecryptedAttachment;
pub use messages::{DecryptedMessageBody, ParsedHeaderValue};

use crate::db::proton_sqlite3::{InProcessTrackerService, Observable};
use crate::db::{
    LabelCountsQuery, LocalAttachmentId, LocalConversationId, LocalLabel, LocalLabelId,
    LocalMessageId,
};
use crate::exports::tracing;
use crate::exports::tracing::debug;
use crate::{MailContextError, MailUserContext, MailUserContextInitializationCallback};
use proton_api_mail::domain::{LabelId, MailSettingsViewMode};
use proton_api_mail::exports::anyhow;
use proton_api_mail::proton_api_core::exports::thiserror;
use proton_api_mail::proton_api_core::exports::tracing::error;
use proton_api_mail::proton_api_core::http::RequestError;
use proton_crypto_inbox::attachment::AttachmentDecryptionError;

pub const DEFAULT_CONVERSATION_COUNT: usize = 50;

#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    #[error("Could not find label with id '{0}'")]
    LabelNotFound(LocalLabelId),
    #[error("Could not find label with remote id '{0}'")]
    RemoteLabelNotFound(LabelId),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(LocalLabelId),
    #[error("Attachment '{0}' not found")]
    AttachmentNotFound(LocalAttachmentId),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(#[from] AttachmentDecryptionError),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryptionIO(String),
    #[error("Conversation '{0}' not found")]
    ConversationNotFound(LocalConversationId),
    #[error("Conversation '{0}' does not have a remote id")]
    ConversationDoesNotHaveRemoteId(LocalConversationId),
    #[error("Message '{0}' does not have a remote id")]
    MessageDoesNotHaveRemoteId(LocalMessageId),
    #[error("Problem with conversation with local ID: '{0}'")]
    ConversationError(LocalConversationId),
    #[error("API request failed with error: '{0}'")]
    APIError(RequestError),
    #[error("{0}")]
    Context(
        #[from]
        #[source]
        MailContextError,
    ),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_action_queue::QueueError),
    #[error("Mailbox is not in the right view mode for the current operation")]
    InvalidViewMode,
    #[error("Action is not valid: {0}")]
    InvalidAction(anyhow::Error),
    #[error("Database Error: {0}")]
    DB(#[from] crate::db::DBError),
    #[error("Message decryption error: {0}")]
    MessageDecryption(#[from] proton_crypto_inbox::message::MessageError),
}

/// Abstraction trait to make it easier to integrate mail in different target platforms. E.g.:
/// Some platforms are able to use the [`crate::db::proton_sqlite3::LiveQuery`] type and other
/// platform may benefit from a different solution.
pub trait MailboxObservableQueryBuilder<Q: Observable> {
    type Output;

    fn build(self, tracker: InProcessTrackerService, query: Q) -> Self::Output;
}

impl<Q: Observable, R, F: FnOnce(InProcessTrackerService, Q) -> R> MailboxObservableQueryBuilder<Q>
    for F
{
    type Output = R;

    fn build(self, tracker: InProcessTrackerService, query: Q) -> Self::Output {
        (self)(tracker, query)
    }
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
    user_ctx: MailUserContext,
    label_id: LocalLabelId,
    view_mode: MailSettingsViewMode,
}

pub trait MailboxBackgroundResult<T: Send>: Send + Sync {
    fn on_background_result(&self, result: MailboxResult<T>);
}

impl<T: Send, F: Fn(MailboxResult<T>) + Send + Sync> MailboxBackgroundResult<T> for F {
    fn on_background_result(&self, result: MailboxResult<T>) {
        (self)(result);
    }
}

enum LabelIdMode<'a> {
    Local(LocalLabelId),
    Remote(&'a LabelId),
}

impl Mailbox {
    pub fn with_remote_id(user_ctx: MailUserContext, label_id: &LabelId) -> MailboxResult<Self> {
        let (label, view_mode) =
            Self::retrieve_label_and_view_mode(&user_ctx, LabelIdMode::Remote(label_id))?;
        Ok(Self::from_label_and_view_mode(user_ctx, label, view_mode))
    }

    pub fn with_id(user_ctx: MailUserContext, label_id: LocalLabelId) -> MailboxResult<Self> {
        let (label, view_mode) =
            Self::retrieve_label_and_view_mode(&user_ctx, LabelIdMode::Local(label_id))?;
        Ok(Self::from_label_and_view_mode(user_ctx, label, view_mode))
    }

    fn from_label_and_view_mode(
        user_ctx: MailUserContext,
        label: LocalLabel,
        view_mode: MailSettingsViewMode,
    ) -> Self {
        let view_mode = label.mail_settings_view_mode().unwrap_or(view_mode);
        debug!("Creating Mailbox ({}, view_mode={:?})", label.id, view_mode);
        Self {
            label_id: label.id,
            view_mode,
            user_ctx,
        }
    }

    fn retrieve_label_and_view_mode(
        user_context: &MailUserContext,
        label_id: LabelIdMode,
    ) -> MailboxResult<(LocalLabel, MailSettingsViewMode)> {
        user_context.db_read(|conn| {
            let label = match label_id {
                LabelIdMode::Local(id) => {
                    let Some(label) = conn.label_with_id(id)? else {
                        return Err(MailboxError::LabelNotFound(id));
                    };
                    label
                }
                LabelIdMode::Remote(id) => {
                    let Some(label) = conn.label_with_remote_id(id)? else {
                        return Err(MailboxError::RemoteLabelNotFound(id.clone()));
                    };
                    label
                }
            };
            let view_mode = conn.mail_settings_view_mode()?;
            Ok((label, view_mode))
        })
    }

    pub fn user_context(&self) -> &MailUserContext {
        &self.user_ctx
    }
    pub fn label_id(&self) -> LocalLabelId {
        self.label_id
    }

    /// Create a new live query which track the total number and unread number of items present
    /// in the current mailbox.
    ///
    /// The returned values will track either messages or conversations depending on the
    /// current mailbox's view mode.
    ///
    /// # Errors
    /// Return error if the database operation failed.
    pub fn new_label_item_count_query<Builder: MailboxObservableQueryBuilder<LabelCountsQuery>>(
        &self,
        builder: Builder,
    ) -> Result<Builder::Output, MailboxError> {
        Ok(builder.build(
            self.user_ctx.tracker_service().clone(),
            LabelCountsQuery::new(self.label_id, self.view_mode),
        ))
    }

    /// Get the label details associated with this mailbox.
    ///
    /// # Errors
    /// Returns error if db query failed.
    pub fn label(&self) -> MailboxResult<Option<LocalLabel>> {
        Ok(self
            .user_context()
            .db_read(|c| c.label_with_id(self.label_id))?)
    }

    /// The mailbox's current view mode.
    pub fn view_mode(&self) -> MailSettingsViewMode {
        self.view_mode
    }

    pub fn refresh(&self, cb: Box<dyn MailUserContextInitializationCallback>) -> MailboxResult<()> {
        let Some(label) = self.user_ctx.get_label(self.label_id)? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };
        let Some(rid) = label.rid else {
            return Err(MailboxError::LabelDoesNotHaveRemoteId(self.label_id));
        };

        self.user_ctx.initialize(rid, cb);
        Ok(())
    }

    /// Sync the label's messages or conversations.
    ///
    /// Depending on the user's mail settings, this function will either sync the conversations
    /// or the messages of the label.
    ///
    /// # Errors
    /// Returns error if API request or database changes failed.
    pub async fn sync(&self, count: usize) -> MailboxResult<()> {
        let rid = self
            .user_ctx
            .db_read(|conn| conn.remote_label_id_from_local_id(self.label_id))
            .map_err(MailContextError::DB)?;
        if let Some(Some(remote_id)) = rid {
            tracing::debug!("Syncing {}({})", self.label_id, remote_id);
            let ctx = self.user_ctx.clone();

            let initialized = ctx
                .db_read(|conn| match self.view_mode {
                    MailSettingsViewMode::Conversations => {
                        conn.check_if_label_is_initialized_conversations(self.label_id)
                    }
                    MailSettingsViewMode::Messages => {
                        conn.check_if_label_is_initialized_messages(self.label_id)
                    }
                })
                .map_err(|e| {
                    error!("Failed to check if label is initialized: {e}");
                    MailContextError::DB(e)
                })?;
            if initialized {
                tracing::debug!("Label {} already initialized, skipping", self.label_id);
                return Ok(());
            }
            tracing::debug!(
                "Label {} not initialized, fetching (mode={:?})",
                self.label_id,
                self.view_mode
            );

            match self.view_mode {
                MailSettingsViewMode::Conversations => ctx
                    .sync_first_conversation_page(remote_id, count)
                    .await
                    .map_err(|e| {
                        error!("Failed to sync conversations for label: {e}");
                        e
                    }),
                MailSettingsViewMode::Messages => ctx
                    .sync_first_message_page(remote_id, count)
                    .await
                    .map_err(|e| {
                        error!("Failed to sync messages for label: {e}");
                        e
                    }),
            }?;

            ctx.db_write(|tx| {
                match self.view_mode {
                    MailSettingsViewMode::Conversations => {
                        tx.mark_label_as_initialized_conversations(self.label_id)?;
                    }
                    MailSettingsViewMode::Messages => {
                        tx.mark_label_as_initialized_messages(self.label_id)?;
                    }
                }
                Ok(())
            })
            .map_err(|e| {
                error!("Failed to mark label as initialized: {e}");
                MailContextError::DB(e)
            })?;

            tracing::debug!("Syncing finished");
            Ok(())
        } else {
            Err(MailboxError::LabelDoesNotHaveRemoteId(self.label_id))
        }
    }
}
