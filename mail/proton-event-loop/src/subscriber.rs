use proton_api_rs::domain::Event;
use proton_api_rs::exports::anyhow;
use proton_api_rs::exports::thiserror;
use proton_async::async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum SubscriberError {
    #[error("{0}")]
    Http(proton_api_rs::http::Error),
    #[error("{0}")]
    Other(anyhow::Error),
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Subscriber: Send + Sync {
    fn name(&self) -> &str;
    async fn on_events(&self, event: &[Event]) -> Result<(), SubscriberError>;
}
