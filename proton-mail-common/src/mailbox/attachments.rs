use std::io;

use crate::{MailContextError, MailUserContext, Mailbox, MailboxError, MailboxResult};
use proton_api_mail::domain::{Attachment, AttachmentMetadata};
use proton_crypto_inbox::proton_crypto::crypto::{
    PGPProvider, PGPProviderSync, VerificationResult,
};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use stash::orm::Model;

/// A decrypted attachment returned by [`Mailbox::load_attachment_to_buffer`].
#[derive(Debug)]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: AttachmentMetadata,
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
        attachment_id: u64,
    ) -> MailboxResult<DecryptedAttachment> {
        let user_context = self.user_context();
        let mut attachment = Attachment::load(attachment_id, user_context.stash())
            .await?
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
        let remote_attachment_id = attachment
            .remote_id
            .clone()
            .ok_or(MailboxError::AttachmentDoesNotHaveRemoteId(attachment_id))?;
        // First check if the metadata is complete for decryption.
        if !attachment.has_complete_metadata() {
            attachment
                .sync_complete_metadata(&user_context.mail_session())
                .await
                .map_err(MailContextError::from)?;
            // Load the complete attachment metadata.
            attachment = Attachment::load(attachment_id, user_context.stash())
                .await?
                .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
        }

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
            attachment,
            user_context,
            attachment_source_reader.as_ref(),
        )
        .await
    }
}

/// Helper function to decrypt the attachment.
async fn decrypt_attachment_to_buffer<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    attachment_info: Attachment,
    mail_user_ctx: &MailUserContext,
    data: impl io::Read,
) -> MailboxResult<DecryptedAttachment> {
    let mut result_buffer: Vec<u8> =
        Vec::with_capacity(attachment_info.size.try_into().unwrap_or_default());

    let signature_verification = {
        let address_keys = mail_user_ctx
            .unlocked_address_keys_async(pgp_provider, &attachment_info.address_id)
            .await?;

        // TODO: Load the sender verification keys for correct signature verification.
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
