use crate::core::datatypes::Id;
use crate::mail::datatypes::{AttachmentMetadata, MessageAddress, MimeType};
use crate::mail::{MailSessionError, MailUserSession};
use crate::uniffi_async;
use proton_mail_common::datatypes::AttachmentMetadata as RealAttachmentMetadata;
use proton_mail_common::draft::{Draft as RealDraft, ReplyMode};
use proton_mail_common::MailContextError;

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
    draft: RealDraft,
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
            let queue_output = match create_mode {
                DraftCreateMode::Empty => RealDraft::action_create_empty(ctx.queue()).await,
                DraftCreateMode::Reply(id) => {
                    RealDraft::action_create_reply(ctx.queue(), ReplyMode::Sender, id.into()).await
                }
                DraftCreateMode::ReplyAll(id) => {
                    RealDraft::action_create_reply(ctx.queue(), ReplyMode::All, id.into()).await
                }
                DraftCreateMode::Forward(id) => {
                    RealDraft::action_create_reply(ctx.queue(), ReplyMode::Forward, id.into()).await
                }
            }
            .map_err(MailContextError::from)?;

            Ok(Self {
                draft: queue_output.local,
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
                draft: RealDraft::open(&ctx, message_id.into()).await?,
            })
        })
        .await
    }

    /// Get the sender of the draft.
    pub fn sender(&self) -> MessageAddress {
        self.draft.sender.clone().into()
    }

    /// Get the To recipients of the draft.
    pub fn to_recipients(&self) -> Vec<MessageAddress> {
        self.draft
            .to_list
            .clone()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Get the To recipients of the draft.
    pub fn cc_recipients(&self) -> Vec<MessageAddress> {
        self.draft
            .cc_list
            .clone()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Get the To recipients of the draft.
    pub fn bcc_recipients(&self) -> Vec<MessageAddress> {
        self.draft
            .bcc_list
            .clone()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Get the draft's subject.
    pub fn subject(&self) -> String {
        self.draft.subject.clone()
    }

    /// Get the draft's body.
    pub fn body(&self) -> String {
        self.draft.body.clone()
    }

    /// Get the draft's attachments
    pub fn attachments(&self) -> Vec<AttachmentMetadata> {
        self.draft
            .attachments
            .clone()
            .into_iter()
            .map(|v| RealAttachmentMetadata::from(v).into())
            .collect()
    }

    /// Get the draft's body mime type.
    pub fn mime_type(&self) -> MimeType {
        self.draft.mime_type.into()
    }
}
