use crate::mail::MailUserSession;
use anyhow::anyhow;
use proton_api_core::service::ApiServiceError;
use proton_event_loop::subscriber::SubscriberError;
use proton_event_loop::EventLoopError as RealEventLoopError;
use tokio::spawn;

#[uniffi::export]
impl MailUserSession {
    /// Poll Event loop and apply events.
    ///
    /// *NOTE*: do not call this function concurrently.
    pub async fn poll_events(&self) -> Result<(), EventLoopError> {
        let ctx = self.ctx.clone();
        let handle = spawn(async move {
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
    Provider(ApiServiceError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl From<RealEventLoopError> for EventLoopError {
    fn from(value: RealEventLoopError) -> Self {
        match value {
            RealEventLoopError::StoreRead(e) => EventLoopError::StoreRead(e),
            RealEventLoopError::StoreWrite(e) => EventLoopError::StoreWrite(e),
            RealEventLoopError::Provider(e) => EventLoopError::Provider(e),
            RealEventLoopError::Subscriber(s, e) => EventLoopError::Subscriber(s, e),
            RealEventLoopError::Other(s) => EventLoopError::Other(anyhow!(s)),
        }
    }
}
