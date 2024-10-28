use crate::core::datatypes::Id;
use crate::mail::datatypes::{AttachmentMetadata, MimeType};
use crate::mail::{MailSessionError, MailUserSession};
use crate::uniffi_async;
use parking_lot::RwLock;
use proton_mail_common::datatypes::AttachmentMetadata as RealAttachmentMetadata;
use proton_mail_common::draft::{Draft as RealDraft, ReplyMode};
use proton_mail_common::{MailContextError, MailUserContext};
use std::sync::Arc;

/// Draft creation mode.
#[derive(Debug, Copy, Clone, uniffi::Enum)]
pub enum DraftCreateMode {
    /// Empty, new message.
    Empty,
    /// Reply to the sender of a message.
    Reply(Id),
    /// Reply to all recipients of a message and the sender.
    ReplyAll(Id),
    /// Forward the message to
    Forward(Id),
}

/// Represents a draft message which can be crafted as empty or as a reply/forward
/// to an existing message.
#[derive(uniffi::Object)]
pub struct Draft {
    draft: RwLock<RealDraft>,
    ctx: Arc<MailUserContext>,
}

#[uniffi::export]
impl Draft {
    /// Create a new draft with the given `create_mode`.
    ///
    /// # Errors
    ///
    /// Return error if action failed.
    #[uniffi::constructor]
    pub async fn new(
        session: &MailUserSession,
        create_mode: DraftCreateMode,
    ) -> Result<Self, MailSessionError> {
        let ctx = session.ctx();
        uniffi_async(async move {
            let draft = match create_mode {
                DraftCreateMode::Empty => RealDraft::empty(ctx.user_stash()).await,
                DraftCreateMode::Reply(id) => {
                    RealDraft::reply(&ctx, id.into(), ReplyMode::Sender, false).await
                }
                DraftCreateMode::ReplyAll(id) => {
                    RealDraft::reply(&ctx, id.into(), ReplyMode::All, false).await
                }
                DraftCreateMode::Forward(id) => {
                    RealDraft::reply(&ctx, id.into(), ReplyMode::Forward, false).await
                }
            }
            .map_err(MailContextError::from)?;

            Ok(Self {
                draft: RwLock::new(draft),
                ctx,
            })
        })
        .await
    }

    #[uniffi::constructor]
    /// Open an existing draft with `message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed or the message is not a draft.
    pub async fn open(session: &MailUserSession, message_id: Id) -> Result<Self, MailSessionError> {
        let ctx = session.ctx();
        uniffi_async(async move {
            Ok(Self {
                draft: RwLock::new(RealDraft::open(&ctx, message_id.into()).await?),
                ctx,
            })
        })
        .await
    }

    /// Get the sender of the draft.
    pub fn sender(&self) -> String {
        self.draft.read().sender.clone()
    }

    /// Get the To recipients of the draft.
    pub fn to_recipients(&self) -> Vec<String> {
        self.draft.read().to_list.clone()
    }

    /// Get the To recipients of the draft.
    pub fn cc_recipients(&self) -> Vec<String> {
        self.draft.read().cc_list.clone()
    }

    /// Get the To recipients of the draft.
    pub fn bcc_recipients(&self) -> Vec<String> {
        self.draft.read().bcc_list.clone()
    }

    /// Get the draft's subject.
    pub fn subject(&self) -> String {
        self.draft.read().subject.clone()
    }

    /// Get the draft's body.
    pub fn body(&self) -> String {
        self.draft.read().body.clone()
    }

    /// Get the draft's attachments
    pub fn attachments(&self) -> Vec<AttachmentMetadata> {
        self.draft
            .read()
            .attachments
            .clone()
            .into_iter()
            .map(|v| RealAttachmentMetadata::from(v).into())
            .collect()
    }

    /// Get the draft's body mime type.
    pub fn mime_type(&self) -> MimeType {
        self.draft.read().mime_type.into()
    }

    /// Save the current draft.
    ///
    /// Schedules an action to create or save the current draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn save(&self) -> Result<(), MailSessionError> {
        let action = {
            let draft = self.draft.read();
            draft.to_save_action()
        };
        let ctx = Arc::clone(&self.ctx);
        uniffi_async(async move {
            ctx.queue()
                .queue_action(action)
                .await
                .map_err(MailContextError::from)?;
            Ok(())
        })
        .await
    }
}
