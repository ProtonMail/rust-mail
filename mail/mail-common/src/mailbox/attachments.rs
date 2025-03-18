use crate::datatypes::{AttachmentMetadata, LocalAttachmentId};
use crate::models::Attachment;
use crate::{
    AppError, MailContextError, MailContextResult, MailUserContext, MailboxError, MailboxResult,
};
use anyhow::Context as _;
use indoc::indoc;
use proton_core_common::os::safe_write_async;
use proton_crypto_inbox::attachment::DecryptableAttachment;
use proton_crypto_inbox::proton_crypto::crypto::{
    PGPProvider, PGPProviderSync, VerificationResult,
};
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use stash::exports::SqliteError;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError};
use std::io::Read;
use std::path::PathBuf;
use tokio::fs;
use tracing::error;

/// A decrypted attachment returned by [`Mailbox::get_attachment`].
#[derive(Debug)]
#[cfg_attr(any(test, debug_assertions), derive(Eq, PartialEq))]
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

impl MailUserContext {
    /// Tries to get where an attachment is stored.
    ///
    /// a. Try to read it from the cache
    /// b.
    ///     - Download it if it can't find it
    ///     - Save it to disk
    ///     - Return the path as a String
    pub async fn get_attachment_content_path(
        &self,
        attachment: &Attachment,
    ) -> MailContextResult<String> {
        let mut tether = self.user_stash().connection();
        let tx = tether.transaction().await?;
        if let Some(path) =
            Self::get_attachment_from_cache(attachment.local_id.unwrap(), &tx).await?
        {
            tx.commit().await?;
            return Ok(path);
        };
        tx.commit().await?;

        let data = self.fetch_attachment_data(attachment).await?;

        // While we were downlaoding, did someone win the race?
        // If so return it. Else store it.
        // TODO(orion): Replace this
        let tx = tether.transaction().await?;
        if let Some(path) =
            Self::get_attachment_from_cache(attachment.local_id.unwrap(), &tx).await?
        {
            tx.commit().await?;
            return Ok(path);
        };

        let at = self
            .store_attachment_in_cache(
                &attachment.filename,
                attachment.local_id.unwrap(),
                data,
                &tx,
            )
            .await?;
        tx.commit().await?;
        Ok(at)
    }

    /// Tries to get the actual bytes of an attachment.
    ///
    /// a. Try to read it from the cache
    /// b.
    ///     - Download it if it can't find it
    ///     - Save it to disk
    ///     - Return the data.
    pub async fn get_attachment_content_data(
        &self,
        attachment: &Attachment,
    ) -> MailContextResult<Vec<u8>> {
        let mut tether = self.user_stash().connection();
        let tx = tether.transaction().await?;
        if let Some(path) =
            Self::get_attachment_from_cache(attachment.local_id.unwrap(), &tx).await?
        {
            tx.commit().await?;
            return Ok(fs::read(path).await?);
        };
        tx.commit().await?;

        let data = self.fetch_attachment_data(attachment).await?;

        // While we were downlaoding, did someone win the race?
        // If so return it. Else store it.
        // TODO(orion): Replace this
        let tx = tether.transaction().await?;
        if let Some(path) =
            Self::get_attachment_from_cache(attachment.local_id.unwrap(), &tx).await?
        {
            return Ok(fs::read(path).await?);
        };

        if let Err(e) = self
            .store_attachment_in_cache(
                &attachment.filename,
                attachment.local_id.unwrap(),
                data.clone(),
                &tx,
            )
            .await
        {
            error!("Could not save attachment to disk/database, but will continue: {e:?}");
        }

        tx.commit().await?;
        Ok(data)
    }

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
        attachment_id: LocalAttachmentId,
    ) -> MailboxResult<DecryptedAttachment> {
        let attachment = self.sync_attachment(attachment_id).await?;
        let data_path = self.get_attachment_content_path(&attachment).await?;
        Ok(DecryptedAttachment {
            attachment_metadata: AttachmentMetadata {
                local_id: Some(attachment_id),
                attachment_type: attachment.attachment_type,
                disposition: attachment.disposition,
                mime_type: attachment.mime_type,
                filename: attachment.filename,
                size: attachment.size,
            },
            data_path: data_path.into(),
        })
    }

    /// Returns a fs path to an attachment in the filesystem.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(tx))]
    pub async fn get_attachment_from_cache(
        id: LocalAttachmentId,
        tx: &Bond<'_>,
    ) -> Result<Option<String>, StashError> {
        let path = tx
            .query_value::<_, String>(
                indoc! {
                    "
                    SELECT path as value FROM attachment_cache
                    WHERE attachment_id = ?1;
                    "
                },
                params![id],
            )
            .await;

        match path {
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Ok(path) => {
                tx.execute(
                    indoc! { "
                       UPDATE attachment_cache
                       SET 
                           atime = unixepoch('now'),
                           hit_count = hit_count + 1
                       WHERE attachment_id = ?1;
"},
                    params![id],
                )
                .await?;
                Ok(Some(path))
            }
            Err(e) => Err(e),
        }
    }

    /// Creates the attachment in the attachment_cache table and stores it in the disk
    /// Returns the path as a String.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(self, data, tether))]
    pub async fn store_attachment_in_cache(
        &self,
        name: &str,
        id: LocalAttachmentId,
        data: Vec<u8>,
        tether: &Bond<'_>,
    ) -> MailContextResult<String> {
        // We will write the attachment to
        // {CACHE_PATH}/attachments/{id}/{name}
        // The reason for this scheme is twofold:
        // - The clients require that the name of the attachment be the name
        // - Two different attachments might share the same name.
        let mut path = self.mail_context().attachments_cache_path();
        path.push(format!("{id}"));
        std::fs::create_dir_all(&path)?;
        path.push(name);
        let path = path
            .into_os_string()
            .into_string()
            // This is infailable since all pieces exist as a string at some point.
            .map_err(MailContextError::InvalidUtf8AttachmentPath)?;

        let data_len = data.len();
        safe_write_async(&path, data).await?;
        tether
            .execute(
                indoc! {
                "INSERT INTO attachment_cache (attachment_id, path, size)
                    VALUES (?, ?, ?)",
                        },
                params![id, path.clone(), data_len],
            )
            .await?;
        Ok(path)
    }

    /// Fetches and decrypts an attachment from the API.
    async fn fetch_attachment_data(&self, attachment: &Attachment) -> MailContextResult<Vec<u8>> {
        let attachment_id = attachment.local_id.expect("Should be set");
        let pgp_provider = new_pgp_provider();
        let remote_attachment_id = match &attachment.attachment_type {
            crate::models::AttachmentType::Remote(Some(id)) => id,
            crate::models::AttachmentType::Remote(None) => {
                return Err(MailContextError::CalledFetchedAttachmentLocalAttachment)
            }
            crate::models::AttachmentType::Pgp => {
                return Err(MailContextError::CalledFetchedAttachmentOnPgp)
            }
        };
        let encrypted_content = Attachment::fetch_content(remote_attachment_id.clone(), self.api())
            .await
            .map_err(|e| {
                error!("Failed to fetch attachment ({attachment_id:?}) from API: {e:?}",);
                e
            })?;
        let (decrypted_content, _verification_result) = self
            .decrypt_attachment_content(&pgp_provider, attachment, encrypted_content.as_ref())
            .await
            // There is not much we can do at this point, and we need to convert from a
            // MailboxError
            .context("Failed to decrypt attachment")
            .map_err(|e| {
                error!("{e:?}");
                MailContextError::Crypto
            })?;
        Ok(decrypted_content)
    }

    /// Sync attachment metadata
    async fn sync_attachment(&self, attachment_id: LocalAttachmentId) -> MailboxResult<Attachment> {
        let mut conn = self.user_stash().connection();
        let mut attachment = Attachment::load(attachment_id, &conn)
            .await
            .inspect_err(|e| {
                error!("Failed to load attachment({attachment_id:?}) from DB: {e:?})")
            })?
            .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
        // First check if the metadata is complete for decryption.
        if !attachment.has_complete_metadata() {
            attachment
                .sync_complete_metadata(self.api(), &mut conn)
                .await
                .inspect_err(|e| {
                    error!("Failed to sync attachment({attachment_id:?}) metadata: {e:?})")
                })
                .map_err(MailContextError::from)?;
            // Load the complete attachment metadata.
            attachment = Attachment::load(attachment_id, &conn)
                .await?
                .ok_or(MailboxError::AttachmentNotFound(attachment_id))?;
        }
        Ok(attachment)
    }

    /// Decrypt attachment content
    async fn decrypt_attachment_content<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        attachment_info: &Attachment,
        data: impl Read,
    ) -> MailboxResult<(Vec<u8>, VerificationResult)> {
        // Can't decrypt with the remote address id.
        let Some(remote_address_id) = &attachment_info.remote_address_id else {
            return Err(
                AppError::AttachmentHasNoAddressId(attachment_info.local_id.unwrap()).into(),
            );
        };

        // Sanity check that the key packets are set, there is an expect() in the decryption
        // code that can trigger application crashes.
        if attachment_info.key_packets.is_none() {
            return Err(
                AppError::AttachmentMissingKeyPackets(attachment_info.local_id.unwrap()).into(),
            );
        }

        let mut result_buffer: Vec<u8> =
            Vec::with_capacity(attachment_info.size.try_into().unwrap_or_default());

        let tether = self.user_stash().connection();

        let address_keys = self
            .unlocked_address_keys(pgp_provider, &tether, remote_address_id)
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
