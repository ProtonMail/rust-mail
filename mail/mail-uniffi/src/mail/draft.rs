mod observer;
mod recipients;

use crate::core::datatypes::Id;
use crate::errors::{DraftError, OptIdDraftResult, VoidDraftResult};
use crate::mail::datatypes::{AttachmentMetadata, MimeType};
use crate::mail::draft::observer::DraftSendResult;
use crate::mail::MailUserSession;
use crate::{async_runtime, uniffi_async};
use parking_lot::RwLock;
use proton_mail_common::datatypes::AttachmentMetadata as RealAttachmentMetadata;
use proton_mail_common::draft::{
    Draft as RealDraft, DraftSaveActionQueuer, DraftSyncStatus as RealDraftSyncStatus, ReplyMode,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::DraftMetadata;
use proton_mail_common::{MailContextError, MailUserContext};
use recipients::ComposerRecipientList;
use std::sync::{Arc, Weak};

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
    ctx: Arc<MailUserContext>,
    to_recipient_list: Arc<ComposerRecipientList>,
    bcc_recipient_list: Arc<ComposerRecipientList>,
    cc_recipient_list: Arc<ComposerRecipientList>,
}
impl Draft {
    fn new_impl(ctx: Arc<MailUserContext>, draft: proton_mail_common::draft::Draft) -> Arc<Self> {
        let to_list = draft.to_list.clone();
        let cc_list = draft.cc_list.clone();
        let bcc_list = draft.bcc_list.clone();
        Arc::new_cyclic(|weak| Self {
            instance: RwLock::new(draft),
            ctx: Arc::clone(&ctx),
            to_recipient_list: ComposerRecipientList::new_to_list(
                Arc::clone(&ctx),
                Weak::clone(weak),
                to_list,
            ),
            bcc_recipient_list: ComposerRecipientList::new_bcc_list(
                Arc::clone(&ctx),
                Weak::clone(weak),
                bcc_list,
            ),
            cc_recipient_list: ComposerRecipientList::new_cc_list(ctx, Weak::clone(weak), cc_list),
        })
    }
}
export_typed_result!(NewDraftResult, Arc<Draft>, DraftError);
export_typed_result!(OpenDraftResult, OpenDraft, DraftError);

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
#[uniffi::export]
pub async fn new_draft(session: &MailUserSession, create_mode: DraftCreateMode) -> NewDraftResult {
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
        .map_err(RealProtonMailError::from)?;

        Result::<_, RealProtonMailError>::Ok(Draft::new_impl(ctx, draft))
    })
    .await
    .map_err(DraftError::from)
    .into()
}

/// Open an existing draft with `message_id`.
///
/// # Errors
///
/// Returns error if the query failed or the message is not a draft.
///
#[uniffi::export]
pub async fn open_draft(session: &MailUserSession, message_id: Id) -> OpenDraftResult {
    let ctx = session.ctx();
    uniffi_async(async move {
        let (draft, status) = RealDraft::open(ctx.clone(), message_id.into()).await?;
        Ok::<_, RealProtonMailError>(OpenDraft {
            draft: Draft::new_impl(ctx, draft),
            sync_status: status.into(),
        })
    })
    .await
    .map_err(DraftError::from)
    .into()
}

#[uniffi::export]
impl Draft {
    /// Get the sender of the draft.
    pub fn sender(&self) -> String {
        self.instance.read().sender.clone()
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
        self.instance.read().subject.clone()
    }

    /// Get the draft's body.
    pub fn body(&self) -> String {
        self.instance.read().body.clone()
    }

    /// Set the draft's `subject`.
    pub fn set_subject(&self, subject: String) -> VoidDraftResult {
        let action = {
            let mut draft = self.instance.write();
            draft.subject = subject;
            draft.to_save_action()
        };
        async_runtime()
            .block_on(async {
                save_draft(&self.ctx, action)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftError::from)
            .into()
    }

    /// Set the draft's `body`.
    pub fn set_body(&self, body: String) -> VoidDraftResult {
        let action = {
            let mut draft = self.instance.write();
            draft.body = body;
            draft.to_save_action()
        };
        async_runtime()
            .block_on(async {
                save_draft(&self.ctx, action)
                    .await
                    .map_err(RealProtonMailError::from)
            })
            .map_err(DraftError::from)
            .into()
    }

    /// Get the draft's attachments
    pub fn attachments(&self) -> Vec<AttachmentMetadata> {
        self.instance
            .read()
            .attachments
            .clone()
            .into_iter()
            .map(|v| RealAttachmentMetadata::from(v).into())
            .collect()
    }

    /// Get the draft's body mime type.
    pub fn mime_type(&self) -> MimeType {
        self.instance.read().mime_type.into()
    }

    /// Get the Draft's message id .
    ///
    /// Returns `None` if no message was created.
    pub async fn message_id(self: Arc<Self>) -> OptIdDraftResult {
        let metadata_id = { self.instance.read().metadata_id };
        let tether = self.ctx.user_stash().connection();
        uniffi_async::<Option<Id>, RealProtonMailError, _>(async move {
            DraftMetadata::message_id(metadata_id, &tether)
                .await
                .map(|v| v.map(Into::into))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(DraftError::from)
        .into()
    }

    /// Retrieve the send result associated with draft.
    ///
    /// Note this only loaded with [`open_draft()`].
    pub fn send_result(&self) -> Option<DraftSendResult> {
        self.instance.read().send_result.clone().map(Into::into)
    }
}

#[uniffi::export]
impl Draft {
    /// Save the current draft.
    ///
    /// Schedules an action to create or save the current draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn save(&self) -> VoidDraftResult {
        let action = {
            let draft = self.instance.read();
            draft.to_save_action()
        };
        let ctx = Arc::clone(&self.ctx);
        uniffi_async(async move {
            ctx.with_queue(|queue| action.queue(queue))
                .await
                .map_err(RealProtonMailError::from)?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftError::from)
        .into()
    }

    /// Sends the draft.
    ///
    /// Schedules an action which saves and then sends the draft.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn send(&self) -> VoidDraftResult {
        let send_queuer = {
            let draft = self.instance.read();
            draft.to_send_action()
        };
        let ctx = Arc::clone(&self.ctx);

        uniffi_async(async move {
            let send_action = send_queuer?;
            ctx.with_queue(|queue| send_action.queue(queue))
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftError::from)
        .into()
    }

    /// Discard the draft.
    ///
    /// Schedules an action which deletes a draft locally and on the server.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn discard(&self) -> VoidDraftResult {
        let discard_queuer = {
            let draft = self.instance.read();
            draft.to_discard_action()
        };
        let ctx = Arc::clone(&self.ctx);

        uniffi_async(async move {
            ctx.with_queue(|queue| discard_queuer.queue(queue))
                .await
                .map_err(RealProtonMailError::from)?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(DraftError::from)
        .into()
    }
}

/// Cancel the sending of message with `message_id`.
///
/// Note that will only work if the message has been sent with a send delay.
#[uniffi::export]
pub async fn draft_undo_send(session: &MailUserSession, message_id: Id) -> VoidDraftResult {
    let ctx = session.ctx();
    uniffi_async(async move {
        ctx.with_queue(|queue| RealDraft::action_undo_send(queue, message_id.into()))
            .await?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(DraftError::from)
    .into()
}

async fn save_draft(
    ctx: &MailUserContext,
    action: DraftSaveActionQueuer,
) -> Result<(), MailContextError> {
    ctx.with_queue(|queue| action.queue(queue))
        .await
        .map_err(MailContextError::from)?;
    Ok(())
}
