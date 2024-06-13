use std::sync::Arc;

use proton_mail_common::db::LocalAttachmentMetadata;

use crate::{
    core::SignatureVerificationResult,
    mail::{Mailbox, MailboxError},
};

/// Returned by [`Mailbox::load_attachment_to_buffer`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: LocalAttachmentMetadata,
    /// The attachment content.
    pub content: Vec<u8>,
    /// The result of the signature verification.
    pub verification_result: Arc<SignatureVerificationResult>,
}

impl From<proton_mail_common::DecryptedAttachment> for DecryptedAttachment {
    fn from(value: proton_mail_common::DecryptedAttachment) -> Self {
        Self {
            attachment_metadata: value.attachment_metadata,
            content: value.content,
            verification_result: Arc::new(value.verification_result.into()),
        }
    }
}

#[uniffi::export]
impl Mailbox {
    /// Loads the plaintext attachment with the given local attachment identifier into the buffer.
    ///
    /// Internally loads the encrypted attachment, decrypts it using the user's matching address keys,
    /// and writes the data into the buffer.
    /// Additionally, attempts to verify any attached signatures with the sender's keys. The result can be accessed via
    /// the `verification_result` field in the [`AttachmentBufferResult`] result type.
    ///
    /// # Warning
    /// Signature verification is currently always failing since no sender keys are fetched yet.
    ///
    /// # Errors
    /// Returns errors if the retrieval or decryption of the attachment fails.
    /// Signature verification failures are not returned as errors.
    pub async fn load_attachment_to_buffer(
        &self,
        local_attachment_id: u64,
    ) -> Result<DecryptedAttachment, MailboxError> {
            self.mbox.load_attachment_to_buffer(local_attachment_id.into())
                .await
                .map(Into::into)
                .map_err(MailboxError::from)
    }
}
