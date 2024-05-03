use std::io;

use crate::db::{LocalAttachment, LocalAttachmentId};
use crate::{MailContextError, MailUserContext, Mailbox, MailboxError, MailboxResult};
use proton_crypto_inbox::attachment::{AttachmentDecryption, InternalAttachmentReader};
use proton_crypto_inbox::proton_crypto::crypto::{
    PGPProvider, PGPProviderSync, VerificationResult,
};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;

impl Mailbox {
    /// Loads the plaintext attachment for the given local [`attachment_id`] into a buffer.
    ///
    /// First loads the encrypted attachment, and then decrypts it into the returned buffer with
    /// the user's address keys.
    /// Additionally, attempts to verify any attached signatures with the sender's keys. The result can be accessed via
    /// the [`VerificationResult`] result return type.
    ///
    /// # Warning
    /// Signature verification is currently always failing since no sender keys are fetched yet.
    ///
    /// # Errors
    /// Returns an error if the encrypted attachment fetching or decryption fails.
    /// Signature verification failures are not returned as errors.
    pub fn load_attachment_to_buffer(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> MailboxResult<(Vec<u8>, VerificationResult)> {
        let user_context = self.user_context();
        let metadata_complete = user_context
            .db_read(|conn| conn.is_attachment_metadata_complete(attachment_id))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
        if !metadata_complete {
            // TODO: Sync metadata
            todo!()
        }

        let attachment_metadata = user_context
            .db_read(|conn| conn.attachment_with_id(attachment_id))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;

        // TODO: Load data attachment data, a reader from a file would be optimal.
        let attachment_source = b"attachment data";
        let attachment_source_reader: &[u8] = attachment_source.as_ref();

        let pgp_provider = new_pgp_provider();
        decrypt_attachment_to_buffer(
            &pgp_provider,
            &attachment_metadata,
            user_context,
            attachment_source_reader,
        )
    }
}

/// Helper function to decrypt the attachment.
fn decrypt_attachment_to_buffer<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    attachment_info: &LocalAttachment,
    mail_user_ctx: &MailUserContext,
    data: impl io::Read,
) -> MailboxResult<(Vec<u8>, VerificationResult)> {
    let mut result_buffer: Vec<u8> =
        Vec::with_capacity(attachment_info.size.try_into().unwrap_or_default());

    let address_keys = mail_user_ctx.address_keys_unlocked(pgp_provider, &attachment_info.address_id)?;

    // TODO: Load the real verification keys in the future.
    let verification_keys: Vec<<Provider as PGPProvider>::PublicKey> = Vec::new();

    let mut decrypting_reader = attachment_info.decrypt_from_reader(
        pgp_provider,
        address_keys.as_ref(),
        &verification_keys,
        data,
    )?;
    std::io::copy(&mut decrypting_reader, &mut result_buffer)
        .map_err(|e| MailboxError::AttachmentDecryptionIO(e.to_string()))?;
    // TODO: Once we have the verification real keys, we should check the signature verification result.
    Ok((result_buffer, decrypting_reader.verification_result()))
}
