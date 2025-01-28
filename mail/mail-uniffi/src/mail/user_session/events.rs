use crate::errors::{EventError, VoidEventResult};
use crate::mail::MailUserSession;
use crate::uniffi_async;
use proton_mail_common::errors::unexpected::Unexpected;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

#[uniffi::export]
impl MailUserSession {
    /// Poll Event loop and apply events.
    ///
    /// *NOTE*: do not call this function concurrently.
    #[allow(clippy::unused_async)]
    pub async fn poll_events(&self) -> VoidEventResult {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            ctx.poll_event_loop()
                .await
                .map_err(|_| RealProtonMailError::Unexpected(Unexpected::Internal))?;
            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(EventError::from)
        .into()
    }
}
