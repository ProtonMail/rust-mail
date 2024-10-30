use crate::errors::update_event::VoidUpdateEventResult;
use crate::mail::MailUserSession;
use crate::uniffi_async;
use proton_mail_common::errors::update_event::UpdateEventError as RealEventLoopError;

#[uniffi::export]
impl MailUserSession {
    /// Poll Event loop and apply events.
    ///
    /// *NOTE*: do not call this function concurrently.
    #[allow(clippy::unused_async)]
    pub async fn poll_events(&self) -> VoidUpdateEventResult {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            ctx.poll_event_loop().await?;
            Result::<_, RealEventLoopError>::Ok(())
        })
        .await
        .into()
    }
}
