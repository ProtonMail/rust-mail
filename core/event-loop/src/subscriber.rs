#![allow(clippy::module_name_repetitions)]

#[cfg(test)]
#[path = "tests/subscriber.rs"]
mod tests;

use async_trait::async_trait;
use flume::{Receiver, Sender};
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

/// Subscriber traits allow anyone to access the events from the event loop.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Subscriber<T: Event + Send + Sync>: Send + Sync {
    /// Return the name/id of this subscriber.
    fn name(&self) -> &str;

    /// Handle incoming events.
    async fn on_events(&self, event: &mut [T]) -> Result<(), SubscriberError>;
}

/// A Subscriber in which all event communication is performed via channels. This may be useful if your subscribe is
/// running on another task and do not wish to make the state sharable.
pub struct ChannelledSubscriber<T: Event + Send + Sync> {
    name: String,
    sender: Sender<Vec<T>>,
    receiver: Receiver<Result<(), SubscriberError>>,
}

#[async_trait]
impl<T: Event + Send + Sync> Subscriber<T> for ChannelledSubscriber<T> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_events(&self, event: &mut [T]) -> Result<(), SubscriberError> {
        if self.sender.send_async(Vec::from(event)).await.is_err() {
            return Err(SubscriberError::Send);
        }

        let Ok(reply) = self.receiver.recv_async().await else {
            return Err(SubscriberError::Receive);
        };

        reply
    }
}

impl<T: Event> ChannelledSubscriber<T> {
    #[must_use]
    pub fn new(name: String) -> (ChannelledSubscriber<T>, ChanneledSubscriberHandler<T>) {
        let (subscriber_sender, subscriber_receiver) = flume::bounded(1);
        let (handler_sender, handler_receiver) = flume::bounded(1);

        (
            ChannelledSubscriber {
                name,
                sender: handler_sender,
                receiver: subscriber_receiver,
            },
            ChanneledSubscriberHandler {
                receiver: handler_receiver,
                sender: subscriber_sender,
            },
        )
    }
}

/// `ChanneledSubscriberHandler` waits on events to be send over a channel. These can then be consumed by the
/// `handle_events` function.
pub struct ChanneledSubscriberHandler<T: Event> {
    receiver: Receiver<Vec<T>>,
    sender: Sender<Result<(), SubscriberError>>,
}

/// Error returned by `ChanneledSubscriberHandler` which includes errors when receiving events or transmitting
/// replies.
#[derive(Debug, Error)]
pub enum ChanneledSubscriberError {
    /// Failed to receive events from the channel.
    #[error("Failed to receive events from channel")]
    Receive,
    /// Failed to send the reply back to the event loop
    #[error("Failed to send result on channel")]
    Send(Result<(), SubscriberError>),
}
impl<T: Event> ChanneledSubscriberHandler<T> {
    /// Handle the events from the event loop.
    ///
    /// # Errors
    /// Returns error if the subscriber failed to handle the events or the communication over
    /// the channel failed.
    pub async fn handle_events_async<Error: Into<SubscriberError>>(
        &mut self,
        mut f: impl FnMut(&[T]) -> Result<(), Error>,
    ) -> Result<(), ChanneledSubscriberError> {
        let Ok(events) = self.receiver.recv_async().await else {
            return Err(ChanneledSubscriberError::Receive);
        };

        let r = (f)(&events);

        self.reply_async(r.map_err(Into::into)).await
    }

    /// Handle the events from the event loop.
    ///
    /// # Errors
    /// Returns error if the subscriber failed to handle the events or the communication over
    /// the channel failed.
    pub fn handle_events<Error: Into<SubscriberError>>(
        &mut self,
        mut f: impl FnMut(&[T]) -> Result<(), Error>,
    ) -> Result<(), ChanneledSubscriberError> {
        let Ok(events) = self.receiver.recv() else {
            return Err(ChanneledSubscriberError::Receive);
        };

        let r = (f)(&events);

        self.reply(r.map_err(Into::into))
    }

    /// Receive events from event loop.
    /// Note: Each call to `receive` must have an `reply` counter part.
    pub async fn receive_async(&mut self) -> Option<Vec<T>> {
        if let Ok(v) = self.receiver.recv_async().await {
            return Some(v);
        }

        None
    }

    /// Receive events from event loop.
    /// Note: Each call to `receive` must have an `reply` counter part.
    pub fn receive(&mut self) -> Option<Vec<T>> {
        if let Ok(v) = self.receiver.recv() {
            return Some(v);
        }

        None
    }

    /// Report the result of handling `receive` to the event loop.
    ///
    /// # Errors
    /// Returns error if the reply could not be sent over the channel.
    pub async fn reply_async(
        &self,
        reply: Result<(), SubscriberError>,
    ) -> Result<(), ChanneledSubscriberError> {
        if let Err(e) = self.sender.send_async(reply).await {
            return Err(ChanneledSubscriberError::Send(e.0));
        }

        Ok(())
    }

    /// Report the result of handling `receive` to the event loop.
    ///
    /// # Errors
    /// Returns error if the reply could not be sent over the channel.
    pub fn reply(
        &self,
        reply: Result<(), SubscriberError>,
    ) -> Result<(), ChanneledSubscriberError> {
        if let Err(e) = self.sender.send(reply) {
            return Err(ChanneledSubscriberError::Send(e.0));
        }

        Ok(())
    }
}
