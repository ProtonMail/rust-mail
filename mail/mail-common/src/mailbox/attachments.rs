use crate::cache::CacheAttachmentKey;
use crate::datatypes::AttachmentMetadata;
use crate::models::Attachment;
use crate::{AppError, MailContextError, Mailbox, MailboxError, MailboxResult};
use anyhow::anyhow;
use proton_api_core::session::CoreSession;
use proton_core_common::cache::{CacheError, CacheResult};
use proton_core_common::datatypes::LocalId;
use proton_crypto_inbox::attachment::DecryptableAttachment;
use proton_crypto_inbox::proton_crypto::crypto::{
    PGPProvider, PGPProviderSync, VerificationResult,
};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use stash::orm::Model;
use std::io::Read;
use std::path::PathBuf;
use tracing::error;

/// A decrypted attachment returned by [`Mailbox::get_attachment`].
#[derive(Debug)]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: AttachmentMetadata,
    /// Content buffer of the attachment
    // TODO: it's ok on mobile to have decrypted attachments in file system. However it's not the
    //       case for desktop. So add an alternative code (behind a feature) later to handle
    //       attachment differently:
    //         * Cache crypted data
    //         * Decrypt
    //         * Add an alternative to this field like `pub content: Vec<u8>`
    pub data_path: PathBuf,
    // /// The result of the signature verification.
    // pub verification_result: VerificationResult,
}

impl Mailbox {
    /// Loads the metadata and file path for the given local [`attachment_id`]
    /// into a [`DecryptedAttachment`].
    ///
    /// If the attachment is not present on the device it is retrieved from
    /// the server, decrypted and stored in the cache.
    ///
    /// Additionally, attempts to verify any attached signatures with the
    /// sender's keys. The result can be accessed via the [`VerificationResult`]
    /// result return type.
    ///
    /// # Warning
    ///
    /// Signature verification is currently always failing since no sender keys
    /// are fetched yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the encrypted attachment fetching or decryption fails.
    /// Signature verification failures are not returned as errors.
    pub async fn get_attachment(
        &self,
        attachment_id: LocalId,
    ) -> MailboxResult<DecryptedAttachment> {
        let attachment = self.sync_attachment(attachment_id).await?;
        let data_path = self.get_attachment_content(&attachment).await?;
        Ok(DecryptedAttachment {
            attachment_metadata: AttachmentMetadata {
                local_id: Some(attachment_id),
                remote_id: attachment.remote_id,
                disposition: attachment.disposition,
                mime_type: attachment.mime_type,
                filename: attachment.filename,
                size: attachment.size,
            },
            data_path,
        })
    }

    /// Get decrypted attachment content
    ///
    /// Content is cached in file system
    pub async fn get_attachment_content(&self, attachment: &Attachment) -> MailboxResult<PathBuf> {
        let user_context = self.user_context();
        let cache = user_context.attachements_cache();
        let key = CacheAttachmentKey::from(attachment);

        Ok(cache
            .get_path_or_insert(&key, self.store_attachment(attachment))
            .await?)
    }

    /// Fetch and store Attachment data
    async fn store_attachment(&self, attachment: &Attachment) -> CacheResult<Vec<u8>> {
        let attachment_id = attachment.local_id.expect("Should be set");
        let user_context = self.user_context();
        let pgp_provider = new_pgp_provider();
        let remote_attachment_id =
            attachment
                .remote_id
                .clone()
                .ok_or(CacheError::Callback(anyhow!(
                    "Attachment without RemoteId {attachment_id}"
                )))?;
        let encrypted_content =
            Attachment::fetch_content(remote_attachment_id.clone(), user_context.session().api())
                .await
                .map_err(|e| {
                    error!("Failed to fetch attachment({attachment_id}) from API: {e})");
                    CacheError::Callback(anyhow!(e))
                })?;
        let (decrypted_content, _verification_result) = self
            .decrypt_attachment(&pgp_provider, attachment, encrypted_content.as_ref())
            .await
            .map_err(|e| {
                error!("Failed to decrypt attachment({attachment_id}): {e})");
                CacheError::Callback(anyhow!(e))
            })?;
        Ok(decrypted_content)
    }

    /// Sync attachment metadata
    async fn sync_attachment(&self, attachment_id: LocalId) -> MailboxResult<Attachment> {
        let user_context = self.user_context();
        let mut attachment = Attachment::load(attachment_id, user_context.user_stash())
            .await
            .inspect_err(|e| error!("Failed to load attachment({attachment_id}) from DB: {e})"))?
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
        // First check if the metadata is complete for decryption.
        if !attachment.has_complete_metadata() {
            attachment
                .sync_complete_metadata(user_context.session().api(), self.stash())
                .await
                .inspect_err(|e| {
                    error!("Failed to sync attachment({attachment_id}) metadata: {e})")
                })
                .map_err(MailContextError::from)?;
            // Load the complete attachment metadata.
            attachment = Attachment::load(attachment_id, user_context.user_stash())
                .await?
                .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
        }
        Ok(attachment)
    }

    /// Decrypt attachment content
    async fn decrypt_attachment<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        attachment_info: &Attachment,
        data: impl Read,
    ) -> MailboxResult<(Vec<u8>, VerificationResult)> {
        let Some(remote_address_id) = &attachment_info.remote_address_id else {
            Err(AppError::Other(anyhow::anyhow!(
                "Attachment has no address id"
            )))?
        };

        let user_context = self.user_context();

        let mut result_buffer: Vec<u8> =
            Vec::with_capacity(attachment_info.size.try_into().unwrap_or_default());

        let address_keys = user_context
            .unlocked_address_keys(pgp_provider, remote_address_id)
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
        let signature_verification = decrypting_reader.verification_result();
        Ok((result_buffer, signature_verification))
    }
}
