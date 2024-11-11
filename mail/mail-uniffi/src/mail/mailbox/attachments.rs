use crate::core::datatypes::Id;
use crate::errors::{MailErrorKind, ProtonMailError};
use crate::mail::datatypes::AttachmentMetadata;
use crate::mail::Mailbox;
use crate::uniffi_async;
use proton_mail_common::errors::MailErrorDetails as RealMailErrorDetails;

/// Returned by [`Mailbox::get_attachment`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: AttachmentMetadata,
    /// The attachment content.
    pub data_path: String,
    // /// The result of the signature verification.
    // pub verification_result: Arc<SignatureVerificationResult>,
}

impl From<proton_mail_common::DecryptedAttachment> for DecryptedAttachment {
    fn from(value: proton_mail_common::DecryptedAttachment) -> Self {
        Self {
            attachment_metadata: value.attachment_metadata.into(),
            data_path: value.data_path.to_str().expect("valid path").to_owned(),
            //            verification_result: Arc::new(value.verification_result.into()),
        }
    }
}

#[proton_uniffi_macros::export_result]
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
        local_attachment_id: Id,
    ) -> Result<DecryptedAttachment, ProtonMailError> {
        let mbox = self.mbox.clone();
        uniffi_async(async move {
            mbox.get_attachment(local_attachment_id.into())
                .await
                .map(Into::into)
                .map_err(RealMailErrorDetails::from)
        })
        .await
        .map_err(|details| MailErrorKind::UserActionError.with(details))
    }
}
