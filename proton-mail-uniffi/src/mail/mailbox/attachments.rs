use std::sync::Arc;

use crate::{
    core::SignatureVerificationResult,
    mail::{Mailbox, MailboxError},
};

/// Returned by [`Mailbox::load_attachment_to_buffer`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct AttachmentBufferResult {
    /// The attachment content.
    pub content: Vec<u8>,
    /// The result of the signature verification.
    pub verification_result: Arc<SignatureVerificationResult>,
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
    pub fn load_attachment_to_buffer(
        &self,
        local_attachment_id: u64,
    ) -> Result<AttachmentBufferResult, MailboxError> {
        self.mbox
            .load_attachment_to_buffer(local_attachment_id.into())
            .map(|(content, verification_result)| AttachmentBufferResult {
                content,
                verification_result: Arc::new(verification_result.into()),
            })
            .map_err(MailboxError::from)
    }
}
