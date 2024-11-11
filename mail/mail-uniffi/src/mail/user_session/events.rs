use crate::errors::{MailErrorKind, VoidProtonMailResult};
use crate::mail::MailUserSession;
use crate::uniffi_async;
use proton_mail_common::errors::MailErrorDetails as RealMailErrorDetails;

#[uniffi::export]
impl MailUserSession {
    /// Poll Event loop and apply events.
    ///
    /// *NOTE*: do not call this function concurrently.
    #[allow(clippy::unused_async)]
    pub async fn poll_events(&self) -> VoidProtonMailResult {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            ctx.poll_event_loop().await?;
            Result::<_, RealMailErrorDetails>::Ok(())
        })
        .await
        .map_err(|details| MailErrorKind::UpdateEventError.with(details))
        .into()
    }
}
