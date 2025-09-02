use crate::core::datatypes::Id;
use crate::errors::ActionError;
use crate::mail::{DecryptedAttachment, Mailbox};
use crate::uniffi_async;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::Attachment;

#[uniffi_export]
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
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let mut tether = ctx.user_stash().connection().await?;
            Attachment::get_attachment(&ctx, local_attachment_id.into(), &mut tether)
                .await
                .map(DecryptedAttachment::try_from)?
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(ActionError::from)
    }
}
