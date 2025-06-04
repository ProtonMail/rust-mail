//! Utilities to listen to the proton event loop. This crate provides an event polling system
//! through the parametrized [`EventPoll<T>`] which is the main entry point to this crate.
//! Handling of events is delegated to a [`Subscriber`]. These need to be registered with the poll.
//!
//! # Event Polling
//!
//! The event polling system requires the user to poll the loop manually so that it can progress.
//! The user is fully responsible for handling errors at the poll call site.
//!
//! ## Example
//!
//! ```ignore
//! use proton_event_loop::{Event, EventPoll, Provider, Store, Subscriber};
//!
//! async fn create_poll_and_run<T: Event>(
//!     store: Box<dyn Store>,
//!     provider: Box<dyn Provider>,
//!     subscriber: Box<dyn Subscriber<T>>
//! ) {
//!     let event_poll = EventPoll::new(store, provider);
//!
//!     // Initialize the poll to set up the initial event ID if needed
//!     event_poll.initialize().await?;
//!
//!     // Register subscriber to handle events
//!     event_poll.register(subscriber).await?;
//!
//!     loop {
//!         if let Err(_) = event_poll.poll().await {
//!             // Handle error
//!         }
//!     }
//! }
//! ```
//!
pub mod poll;
pub mod provider;
pub mod store;
pub mod subscriber;

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;

// Re-export main types
pub use poll::EventPoll;
pub use subscriber::{RawSubscriber, Subscriber, TypedSubscribers};

use crate::subscriber::SubscriberError;
use anyhow::Error as AnyhowError;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;
use serde::Deserialize;
use serde_with::{BoolFromInt, serde_as};
use std::fmt::Debug;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventLoopError {
    #[error("Subscriber ({0}) failed to apply refresh event: {1}")]
    Refresh(String, SubscriberError),
    #[error("Failed to read from store: {0}")]
    StoreRead(AnyhowError),
    #[error("Failed to write store: {0}")]
    StoreWrite(AnyhowError),
    #[error("Failed to retrieve event: {0}")]
    Provider(#[from] ApiServiceError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
    #[error("Subscriber with `{0}` name already exists")]
    Register(&'static str),
}

/// This represents an event returned by the API.
pub trait Event: Clone + Debug + Eq + PartialEq + Send + Sync + 'static {
    /// The API response type of the event.
    type Response: Clone + Debug + for<'de> Deserialize<'de> + Eq + PartialEq + Send + Sync;

    /// Get the event id of the event.
    fn event_id(&self) -> &EventId;

    /// Check if the event has more data.
    fn has_more(&self) -> bool;

    /// Whether this was a refresh event.
    fn is_refresh(&self) -> bool;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RawEvent {
    meta: EventMetadata,
    raw: String,
}

impl RawEvent {
    pub fn from_json(raw: String) -> Result<Self, AnyhowError> {
        Ok(Self {
            meta: serde_json::from_str(&raw)?,
            raw,
        })
    }

    pub fn deserialize<T: Event + From<<T as Event>::Response>>(&self) -> Result<T, AnyhowError> {
        let event = T::from(serde_json::from_str(&self.raw)?);

        Ok(event)
    }
}

impl Event for RawEvent {
    type Response = String;

    fn event_id(&self) -> &EventId {
        &self.meta.event_id
    }

    fn has_more(&self) -> bool {
        self.meta.has_more
    }

    fn is_refresh(&self) -> bool {
        self.meta.refresh != 0
    }
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
struct EventMetadata {
    #[serde(rename = "EventID")]
    event_id: EventId,
    #[serde(rename = "More")]
    #[serde_as(as = "BoolFromInt")]
    has_more: bool,
    refresh: u8,
}
