use std::io;

use crate::db::{LocalAttachment, LocalAttachmentId, LocalAttachmentMetadata};
use crate::{MailContextError, MailUserContext, Mailbox, MailboxError, MailboxResult};
use proton_crypto_inbox::attachment::AttachmentDecryption;
use proton_crypto_inbox::proton_crypto::crypto::{
    PGPProvider, PGPProviderSync, VerificationResult,
};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;

/// A decrypted attachment returned by [`Mailbox::load_attachment_to_buffer`].
#[derive(Debug)]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: LocalAttachmentMetadata,
    /// Content buffer of the attachment
    pub content: Vec<u8>,
    /// The result of the signature verification.
    pub verification_result: VerificationResult,
}

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
    pub async fn load_attachment_to_buffer(
        &self,
        attachment_id: LocalAttachmentId,
    ) -> MailboxResult<DecryptedAttachment> {
        let user_context = self.user_context();
        // First check if the metadata is complete for decryption.
        let (metadata_complete, attachment_id_opt) = user_context
            .db_read(|conn| conn.is_attachment_metadata_complete(attachment_id))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;

        if !metadata_complete {
            let remote_attachment_id =
                attachment_id_opt.ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
            user_context
                .sync_complete_attachment_metadata(remote_attachment_id)
                .await
                .map_err(MailContextError::from)?;
        }

        // Load the complete attachment metadata.
        let attachment_metadata = user_context
            .db_read(|conn| conn.attachment_with_id(attachment_id))
            .map_err(MailContextError::from)?
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;

        let remote_attachment_id = attachment_metadata
            .rid
            .as_ref()
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;

        // Load the attachment content.
        // TODO: Lets opt for a stream in the future
        let attachment_source_reader = user_context
            .mail_session()
            .attachment_content(remote_attachment_id.clone())
            .await
            .map_err(MailContextError::from)?;

        // Decrypt it.
        let pgp_provider = new_pgp_provider();
        decrypt_attachment_to_buffer(
            &pgp_provider,
            attachment_metadata,
            user_context,
            attachment_source_reader.as_ref(),
        )
        .await
    }
}

/// Helper function to decrypt the attachment.
async fn decrypt_attachment_to_buffer<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    attachment_info: LocalAttachment,
    mail_user_ctx: &MailUserContext,
    data: impl io::Read,
) -> MailboxResult<DecryptedAttachment> {
    let mut result_buffer: Vec<u8> =
        Vec::with_capacity(attachment_info.size.try_into().unwrap_or_default());

    let signature_verification = {
        let address_keys = mail_user_ctx
            .address_keys_unlocked_async(pgp_provider, &attachment_info.address_id)
            .await?;

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
        decrypting_reader.verification_result()
    };

    let result = DecryptedAttachment {
        attachment_metadata: attachment_info.into(),
        content: result_buffer,
        verification_result: signature_verification,
    };

    Ok(result)
}
