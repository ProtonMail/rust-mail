use crate::core::datatypes::Id;
use crate::errors::ActionError;
use crate::mail::{DecryptedAttachment, Mailbox};
use crate::uniffi_async;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

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
    ) -> Result<DecryptedAttachment, ActionError> {
        let mbox = self.mbox.clone();
        uniffi_async(async move {
            mbox.user_context()
                .get_attachment(local_attachment_id.into())
                .await
                .map(Into::into)
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(ActionError::from)
    }
}
