mod attachments;
mod observer;
mod recipients;

use crate::core::datatypes::Id;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{
    DraftDiscardError, DraftOpenError, DraftSaveSendError, DraftUndoSendError,
    EmbeddedAttachmentInfoResult, ProtonError, VoidDraftDiscardResult, VoidDraftSaveSendResult,
    VoidDraftUndoSendResult,
};
use crate::mail::MailUserSession;
use crate::mail::datatypes::MimeType;
use crate::mail::draft::attachments::AttachmentList;
use crate::mail::draft::observer::DraftSendResult;
use crate::mail::messages::EmbeddedAttachmentInfo;
use crate::mail::state::MailUserContextPtr;
use crate::{async_runtime, uniffi_async};
use proton_mail_common::datatypes::attachment::ContentId;
use proton_mail_common::draft::{
    Draft as RealDraft, DraftSyncStatus as RealDraftSyncStatus, ReplyMode,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::DraftMetadata;
use proton_mail_common::{MailContextError, MailUserContext};
use recipients::ComposerRecipientList;
use std::sync::{Arc, Weak};
use tokio::sync::RwLock;

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
    instance: RwLock<RealDraft>,
    ctx: MailUserContextPtr,
    to_recipient_list: Arc<ComposerRecipientList>,
    bcc_recipient_list: Arc<ComposerRecipientList>,
    cc_recipient_list: Arc<ComposerRecipientList>,
    attachment_list: Arc<AttachmentList>,
}
impl Draft {
    fn new_impl(
        ctx: MailUserContextPtr,
        real_ctx: &MailUserContext,
        draft: proton_mail_common::draft::Draft,
    ) -> Arc<Self> {
        let to_list = draft.to_list.clone();
        let cc_list = draft.cc_list.clone();
        let bcc_list = draft.bcc_list.clone();
        let staging_path = draft.attachment_staging_path(real_ctx);
        Arc::new_cyclic(|weak| Self {
            instance: RwLock::new(draft),
            ctx: ctx.clone(),
            to_recipient_list: ComposerRecipientList::new_to_list(
                ctx.clone(),
                Weak::clone(weak),
                to_list,
            ),
            bcc_recipient_list: ComposerRecipientList::new_bcc_list(
                ctx.clone(),
                Weak::clone(weak),
                bcc_list,
            ),
            cc_recipient_list: ComposerRecipientList::new_cc_list(ctx, Weak::clone(weak), cc_list),
            attachment_list: AttachmentList::new(&staging_path, Weak::clone(weak)),
        })
    }
}

/// Represent an open draft.
#[derive(uniffi::Record)]
pub struct OpenDraft {
    /// The draft object itself.
    pub draft: Arc<Draft>,
    /// Whether the draft was synced from the server or we are operating on a cached version.
    pub sync_status: DraftSyncStatus,
}

/// Indicates whether the draft was synced from the server or we are operating on a cached version.
#[derive(uniffi::Enum)]
pub enum DraftSyncStatus {
    /// Draft was not synced from server and we are operating on a cached version
    Cached,
    /// Draft was synced from server.
    Synced,
}

impl From<RealDraftSyncStatus> for DraftSyncStatus {
    fn from(value: RealDraftSyncStatus) -> Self {
        match value {
            RealDraftSyncStatus::Synced => Self::Synced,
            RealDraftSyncStatus::Cached => Self::Cached,
        }
    }
}

/// Create a new draft with the given `create_mode`.
///
/// # Errors
///
/// Return error if action failed.
///
#[uniffi_export]
pub async fn new_draft(
    session: &MailUserSession,
    create_mode: DraftCreateMode,
) -> Result<Arc<Draft>, DraftOpenError> {
    let ctx = session.ctx()?;
    let ptr = session.ptr();
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
        .map_err(RealProtonMailError::from)?;

        Result::<_, RealProtonMailError>::Ok(Draft::new_impl(ptr, &ctx, draft))
    })
    .await
    .map_err(DraftOpenError::from)
    .into()
}

/// Open an existing draft with `message_id`.
///
/// # Errors
///
/// Returns error if the query failed or the message is not a draft.
///
#[uniffi_export]
pub async fn open_draft(
    session: &MailUserSession,
    message_id: Id,
) -> Result<OpenDraft, DraftOpenError> {
    let ctx = session.ctx()?;
    let ptr = session.ptr();
    uniffi_async(async move {
        let (draft, status) = RealDraft::open(&ctx, message_id.into()).await?;
        Ok::<_, RealProtonMailError>(OpenDraft {
            draft: Draft::new_impl(ptr, &ctx, draft),
            sync_status: status.into(),
        })
    })
    .await
    .map_err(DraftOpenError::from)
    .into()
}

#[uniffi_export]
impl Draft {
    /// Get the sender of the draft.
    pub fn sender(&self) -> String {
        async_runtime().block_on(async { self.instance.read().await.sender.clone() })
    }

    /// Get the To recipients of the draft.
    pub fn to_recipients(&self) -> Arc<ComposerRecipientList> {
        Arc::clone(&self.to_recipient_list)
    }

    /// Get the Cc recipients of the draft.
    pub fn cc_recipients(&self) -> Arc<ComposerRecipientList> {
        Arc::clone(&self.cc_recipient_list)
    }

    /// Get the Bcc recipients of the draft.
    pub fn bcc_recipients(&self) -> Arc<ComposerRecipientList> {
        Arc::clone(&self.bcc_recipient_list)
    }

    /// Get the draft's subject.
    pub fn subject(&self) -> String {
        async_runtime().block_on(async { self.instance.read().await.subject.clone() })
    }

    /// Get the draft's body.
    pub fn body(&self) -> String {
        async_runtime().block_on(async { self.instance.read().await.body().to_owned() })
    }

    /// Set the draft's `subject`.
    #[returns(VoidDraftSaveSendResult)]
    pub fn set_subject(&self, subject: String) -> Result<(), DraftSaveSendError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftSaveSendError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        async_runtime()
            .block_on(async {
                let mut instance = self.instance.write().await;
                if instance.subject == subject {
                    return Ok(());
                }
                instance.subject = subject;
                save_draft(&ctx, &mut instance)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftSaveSendError::from)
            .into()
    }

    /// Set the draft's `body`.
    #[returns(VoidDraftSaveSendResult)]
    pub fn set_body(&self, body: String) -> Result<(), DraftSaveSendError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftSaveSendError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        async_runtime()
            .block_on(async {
                let mut instance = self.instance.write().await;
                if instance.body() == body {
                    return Ok(());
                }
                instance.set_body(body);
                save_draft(&ctx, &mut instance)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftSaveSendError::from)
            .into()
    }

    /// Get the draft's body mime type.
    pub fn mime_type(&self) -> MimeType {
        async_runtime().block_on(async { self.instance.read().await.mime_type().into() })
    }

    /// Get the Draft's message id .
    ///
    /// Returns `None` if no message was created.
    pub async fn message_id(self: Arc<Self>) -> Result<Option<Id>, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        uniffi_async::<Option<Id>, RealProtonMailError, _>(async move {
            let metadata_id = { self.instance.read().await.metadata_id };
            let tether = ctx.user_stash().connection();
            DraftMetadata::message_id(metadata_id, &tether)
                .await
                .map(|v| v.map(Into::into))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }

    /// Retrieve the send result associated with draft.
    ///
    /// Note this only loaded with [`open_draft()`].
    pub fn send_result(&self) -> Option<DraftSendResult> {
        async_runtime().block_on(async {
            self.instance
                .read()
                .await
                .send_result
                .clone()
                .map(Into::into)
        })
    }

    /// Load an embedded attachment in this draft message.
    ///
    /// See [`DecryptedMessageBody::get_embedded_attachment`] for more details.
    ///
    /// # Errors
    ///
    /// See [`DecryptedMessageBody::get_embedded_attachment`] for more details.
    //NOTE: iOS request we share the same result types between
    // this function and the DecryptedMessageBody equivalent.
    #[returns(EmbeddedAttachmentInfoResult)]
    pub async fn get_embedded_attachment(
        self: Arc<Self>,
        cid: String,
    ) -> Result<EmbeddedAttachmentInfo, ProtonError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ProtonError::Unexpected(UnexpectedError::Internal));
        };
        uniffi_async(async move {
            let draft = self.instance.read().await;
            let att = draft
                .get_embedded_attachment(&ctx, &ContentId::from(cid))
                .await
                .map_err(RealProtonMailError::from)?;
            Ok::<_, RealProtonMailError>(EmbeddedAttachmentInfo {
                data: att.data,
                mime: att.mime,
                height: att.height,
                width: att.width,
            })
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }

    /// Get the attachment list.
    pub fn attachment_list(&self) -> Arc<AttachmentList> {
        Arc::clone(&self.attachment_list)
    }
}

#[uniffi_export]
impl Draft {
    /// Save the current draft.
    ///
    /// Schedules an action to create or save the current draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftSaveSendResult)]
    pub async fn save(self: Arc<Self>) -> Result<(), DraftSaveSendError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftSaveSendError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        uniffi_async(async move {
            let mut instance = self.instance.write().await;
            instance
                .save(ctx.action_queue(), &ctx.user_stash().connection())
                .await
                .map_err(RealProtonMailError::from)?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSaveSendError::from)
        .into()
    }

    /// Sends the draft.
    ///
    /// Schedules an action which saves and then sends the draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftSaveSendResult)]
    pub async fn send(self: Arc<Self>) -> Result<(), DraftSaveSendError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftSaveSendError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        uniffi_async(async move {
            let mut instance = self.instance.write().await;
            instance
                .send(ctx.action_queue(), &ctx.user_stash().connection())
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftSaveSendError::from)
        .into()
    }

    /// Discard the draft.
    ///
    /// Schedules an action which deletes a draft locally and on the server.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[returns(VoidDraftDiscardResult)]
    pub async fn discard(self: Arc<Self>) -> Result<(), DraftDiscardError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(DraftDiscardError::Other(ProtonError::Unexpected(
                UnexpectedError::Internal,
            )));
        };
        uniffi_async(async move {
            let instance = self.instance.read().await;
            instance
                .discard(ctx.action_queue())
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftDiscardError::from)
        .into()
    }
}

/// Cancel the sending of message with `message_id`.
///
/// Note that will only work if the message has been sent with a send delay.
#[uniffi_export]
#[returns(VoidDraftUndoSendResult)]
pub async fn draft_undo_send(
    session: &MailUserSession,
    message_id: Id,
) -> Result<(), DraftUndoSendError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        RealDraft::action_undo_send(ctx.action_queue(), message_id.into()).await?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(DraftUndoSendError::from)
    .into()
}

/// Discard a Draft by with the given `message_id`.
///
/// Note that this requires that the given message interacted with any of the [`Draft`] APIs
/// in the past.
#[uniffi_export]
#[returns(VoidDraftDiscardResult)]
pub async fn draft_discard(
    session: &MailUserSession,
    message_id: Id,
) -> Result<(), DraftDiscardError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let tether = ctx.user_stash().connection();
        RealDraft::action_discard(message_id.into(), &tether, ctx.action_queue()).await?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(DraftDiscardError::from)
    .into()
}

async fn save_draft(ctx: &MailUserContext, draft: &mut RealDraft) -> Result<(), MailContextError> {
    draft
        .save(ctx.action_queue(), &ctx.user_stash().connection())
        .await?;
    Ok(())
}
