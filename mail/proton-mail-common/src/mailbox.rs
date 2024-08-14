mod attachments;

pub mod decrypted_message;

use crate::datatypes::ViewMode;
use crate::mailbox::decrypted_message::DecryptedMessageError;
use crate::models::{Conversation, Label, MailSettings, Message, MAIL_SETTINGS_ID};
use crate::{AppError, MailContextError, MailUserContext, MailUserContextInitializationCallback};
pub use attachments::DecryptedAttachment;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::Proton;
use proton_api_core::session::CoreSession;
use proton_core_common::cache::CacheError;
use proton_core_common::datatypes::{LabelId, LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::attachment::AttachmentDecryptionError;
use stash::orm::Model;
use stash::stash::{Stash, StashError};
use std::sync::Arc;
use tracing::{debug, error};

pub const DEFAULT_CONVERSATION_COUNT: usize = 50;

#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    #[error("Could not find label with local id '{0}'")]
    LabelNotFound(LocalId),
    #[error("Could not find label with remote id '{0}'")]
    RemoteLabelNotFound(LabelId),
    #[error("Label '{0}' does not have a remote id")]
    LabelDoesNotHaveRemoteId(LocalId),
    #[error("No exclusive location found for message '{0}'")]
    NoExclusiveLocationFound(LocalId),
    #[error("Attachment '{0}' not found")]
    AttachmentNotFound(LocalId),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(#[from] AttachmentDecryptionError),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryptionIO(String),
    #[error("Attachment '{0}' does not have a remote id")]
    AttachmentDoesNotHaveRemoteId(LocalId),
    #[error("Conversation '{0}' not found")]
    ConversationNotFound(LocalId),
    #[error("Conversation '{0}' does not have a remote id")]
    ConversationDoesNotHaveRemoteId(LocalId),
    #[error("Message '{0}' does not have a remote id")]
    MessageDoesNotHaveRemoteId(LocalId),
    #[error("Could not find message with local id '{0}'")]
    MessageNotFound(LocalId),
    #[error("Problem with conversation with local ID: '{0}'")]
    ConversationError(LocalId),
    #[error("Conversation '{0}' has no messages")]
    ConversationHasNoMessages(LocalId),
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
    #[error("Mailbox is not in the right view mode for the current operation")]
    InvalidViewMode,
    #[error("Action is not valid: {0}")]
    InvalidAction(anyhow::Error),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Message decryption error: {0}")]
    MessageDecryption(#[from] proton_crypto_inbox::message::MessageError),
    #[error("Decrypted Message: {0}")]
    DecryptedMessage(#[from] DecryptedMessageError),
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),
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
    user_ctx: Arc<MailUserContext>,
    label_id: LocalId,
    view_mode: ViewMode,
}

pub trait MailboxBackgroundResult<T: Send>: Send + Sync {
    fn on_background_result(&self, result: MailboxResult<T>);
}

impl<T: Send, F: Fn(MailboxResult<T>) + Send + Sync> MailboxBackgroundResult<T> for F {
    fn on_background_result(&self, result: MailboxResult<T>) {
        (self)(result);
    }
}

impl Mailbox {
    pub async fn new(user_ctx: Arc<MailUserContext>, label_id: LocalId) -> MailboxResult<Self> {
        let Some(label) = Label::load(label_id, user_ctx.stash()).await? else {
            return Err(MailboxError::LabelNotFound(label_id));
        };
        let view_mode = if let Some(view_mode) = label.view_mode() {
            view_mode
        } else {
            MailSettings::load(MAIL_SETTINGS_ID.into(), user_ctx.stash())
                .await
                .map_err(|e| {
                    error!("Failed to load mail settings: {e}");
                    e
                })
                .ok()
                .and_then(|settings| settings)
                .map(|settings| settings.view_mode)
                .unwrap_or(ViewMode::Conversations)
        };
        debug!("Creating Mailbox ({}, view_mode={:?})", label_id, view_mode);
        Ok(Self {
            label_id,
            view_mode,
            user_ctx,
        })
    }

    pub async fn with_remote_id(
        user_ctx: Arc<MailUserContext>,
        label_id: LabelId,
    ) -> MailboxResult<Self> {
        let label = Label::find_by_id(RemoteId::from(label_id), user_ctx.stash())
            .await?
            .unwrap();
        let view_mode = label.view_mode().unwrap_or(
            MailSettings::load(MAIL_SETTINGS_ID.into(), user_ctx.stash())
                .await?
                .unwrap()
                .view_mode,
        );
        debug!(
            "Creating Mailbox ({}, view_mode={:?})",
            label.local_id.unwrap(),
            view_mode
        );
        Ok(Self {
            label_id: label.local_id.unwrap(),
            view_mode,
            user_ctx,
        })
    }

    /// Get the user context.
    pub fn user_context(&self) -> Arc<MailUserContext> {
        Arc::clone(&self.user_ctx)
    }

    /// Get the API service.
    pub fn api(&self) -> &Proton {
        self.user_ctx.api()
    }

    /// Get the database connection.
    pub fn stash(&self) -> &Stash {
        self.user_ctx.stash()
    }

    pub fn label_id(&self) -> LocalId {
        self.label_id
    }

    pub async fn refresh(
        &self,
        cb: Box<dyn MailUserContextInitializationCallback>,
    ) -> MailboxResult<()> {
        let Some(label) = Label::load(self.label_id, self.user_ctx.stash()).await? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };
        let Some(rid) = label.remote_id else {
            return Err(MailboxError::LabelDoesNotHaveRemoteId(self.label_id));
        };

        self.user_ctx.initialize(rid, cb).await;
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
        let ctx = self.user_ctx.clone();
        let Some(mut label) = Label::load(self.label_id, ctx.stash()).await? else {
            return Err(MailboxError::LabelNotFound(self.label_id));
        };

        if let Some(remote_id) = label.remote_id.clone() {
            debug!("Syncing {}({})", self.label_id, &remote_id);

            let initialized = match self.view_mode {
                ViewMode::Conversations => label.initialized_conv,
                ViewMode::Messages => label.initialized_msg,
            };
            if initialized {
                debug!("Label {} already initialized, skipping", self.label_id);
                return Ok(());
            }
            debug!(
                "Label {} not initialized, fetching (mode={:?})",
                self.label_id, self.view_mode
            );

            match self.view_mode {
                ViewMode::Conversations => Conversation::sync_first_conversation_page(
                    remote_id,
                    count,
                    ctx.session().api(),
                    ctx.stash(),
                )
                .await
                .map_err(|e| {
                    error!("Failed to sync conversations for label: {e}");
                    e
                }),
                ViewMode::Messages => Message::sync_first_message_page(
                    remote_id,
                    count,
                    ctx.session().api(),
                    ctx.stash(),
                )
                .await
                .map_err(|e| {
                    error!("Failed to sync messages for label: {e}");
                    e
                }),
            }?;

            match self.view_mode {
                ViewMode::Conversations => {
                    label.initialized_conv = true;
                }
                ViewMode::Messages => {
                    label.initialized_msg = true;
                }
            }
            label.save().await.map_err(|e| {
                error!("Failed to mark label as initialized: {e}");
                MailContextError::Stash(e)
            })?;

            debug!("Syncing finished");
            Ok(())
        } else {
            Err(MailboxError::LabelDoesNotHaveRemoteId(self.label_id))
        }
    }

    /// The mailbox's current view mode.
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }
}
