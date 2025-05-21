#![allow(clippy::module_name_repetitions)]

#[cfg(test)]
#[path = "tests/subscriber.rs"]
mod tests;

use async_trait::async_trait;
// avoid namespace conflicts
use crate::Event;
use anyhow::Error as AnyhowError;
use proton_core_api::service::ApiServiceError;
use stash::stash::StashError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubscriberError {
    /// API error should be returned when the error resulted due to an API or Network error.
    #[error("{0}")]
    Api(#[from] ApiServiceError),
    /// Subscriber specific errors should be returned here.
    #[error("{0}")]
    Other(AnyhowError),
    /// Failed to send to the subscriber.
    #[error("Failed to send data to subscriber")]
    Send,
    /// Failed to receive data from subscriber.
    #[error("Failed to receive data from subscriber")]
    Receive,
    /// Stash error, i.e. database error.
    #[error("{0}")]
    StashError(#[from] StashError),
}

impl From<AnyhowError> for SubscriberError {
    fn from(value: AnyhowError) -> Self {
        Self::Other(value)
    }
}

/// Subscriber traits allow anyone to access the events from the event loop.
#[async_trait]
pub trait Subscriber<T: Event>: Send + Sync {
    /// Return the name/id of this subscriber.
    fn name(&self) -> &'static str;

    /// Handle incoming events.
    async fn on_events(&self, event: &mut [T]) -> Result<(), SubscriberError>;

    /// Handle refresh event
    async fn on_refresh(&self, event: &T) -> Result<(), SubscriberError>;
}
