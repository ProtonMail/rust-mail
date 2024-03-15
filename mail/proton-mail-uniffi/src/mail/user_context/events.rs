use crate::mail::MailUserContext;
use proton_mail_common::exports::anyhow::anyhow;
use proton_mail_common::exports::proton_event_loop::{EventLoopError as ELError, SubscriberError};
use proton_mail_common::exports::{anyhow, thiserror};
use proton_mail_common::proton_api_mail::proton_api_core::http::HttpRequestError;

#[uniffi::export]
impl MailUserContext {
    /// Poll Event loop and apply events.
    ///
    /// *NOTE*: do not call this function concurrently.
    pub async fn poll_events(&self) -> Result<(), EventLoopError> {
        let ctx = self.ctx.clone();
        let handle = self.ctx.mail_context().async_runtime().spawn(async move {
            ctx.poll_event_loop().await?;
            Ok(())
        });
        handle
            .await
            .map_err(|e| EventLoopError::Other(anyhow::anyhow!("Failed to join task: {e}")))?
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum EventLoopError {
    #[error("Failed to read from store: {0}")]
    StoreRead(anyhow::Error),
    #[error("Failed to write store: {0}")]
    StoreWrite(anyhow::Error),
    #[error("Failed to retrieve event: {0}")]
    Provider(HttpRequestError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl From<ELError> for EventLoopError {
    fn from(value: ELError) -> Self {
        match value {
            ELError::StoreRead(e) => EventLoopError::StoreRead(e),
            ELError::StoreWrite(e) => EventLoopError::StoreWrite(e),
            ELError::Provider(e) => EventLoopError::Provider(e),
            ELError::Subscriber(s, e) => EventLoopError::Subscriber(s, e),
            ELError::Other(s) => EventLoopError::Other(anyhow!(s)),
        }
    }
}
