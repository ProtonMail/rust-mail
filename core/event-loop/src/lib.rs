//! Utilities to listen to the proton event loop. This crate provides an event polling system
//! that can handle multiple event types through the [`EventPoll`] which is the main entry point to this crate.
//!
//! The system works with raw events that are then converted to specific event types by registered subscribers.
//! Handling of events is delegated to [`Subscriber`]s which are automatically wrapped in [`TypedSubscribers`] containers
//! that implement the [`RawSubscriber`] trait.
//!
//! # Event Polling Architecture
//!
//! The event polling system uses a two-tier approach:
//! - **Raw Events**: Events are initially fetched as [`RawEvent`]s from the API
//! - **Typed Events**: Each [`RawSubscriber`] deserializes raw events to specific event types
//!
//! This design allows a single [`EventPoll`] to handle multiple event types (e.g., core events, mail events)
//! without requiring separate polling loops.
//!
//! # Registration System
//!
//! The event poll uses a **type-based registration system**:
//! - Subscribers are grouped by their event type (`TypeId`)
//! - Multiple subscribers for the same event type are automatically grouped together
//! - No need to manually manage subscriber names or collections
//!
//! ## Basic Usage
//!
//! ```ignore
//! use proton_event_loop::{EventPoll, Provider, Store, Subscriber};
//!
//! async fn create_poll_and_run(
//!     store: Box<dyn Store>,
//!     provider: Box<dyn Provider>,
//!     core_subscriber1: Box<dyn Subscriber<CoreEvent>>,
//!     core_subscriber2: Box<dyn Subscriber<CoreEvent>>,
//!     mail_subscriber: Box<dyn Subscriber<MailEvent>>
//! ) -> Result<(), EventLoopError> {
//!     let event_poll = EventPoll::new(store, provider);
//!
//!     // Initialize the poll to set up the initial event ID if needed
//!     event_poll.initialize().await?;
//!
//!     // Register subscribers - they're automatically grouped by event type
//!     event_poll.register(core_subscriber1).await?;  // Creates TypedSubscribers<CoreEvent>
//!     event_poll.register(core_subscriber2).await?;  // Adds to existing TypedSubscribers<CoreEvent>
//!     event_poll.register(mail_subscriber).await?;   // Creates TypedSubscribers<MailEvent>
//!
//!     // Poll for events - all registered subscribers will receive appropriate events
//!     loop {
//!         if let Err(e) = event_poll.poll().await {
//!             // Handle error - detailed error information is provided
//!             eprintln!("Event polling failed: {e}");
//!         }
//!     }
//! }
//! ```
//!
//! ## Key Features
//!
//! - **Automatic Grouping**: Multiple subscribers for the same event type are automatically grouped
//! - **Type Safety**: Registration is compile-time type-safe with `register<T>()`
//! - **Error Handling**: Comprehensive error reporting with context about which subscriber failed
//! - **FIFO Processing**: Subscribers are processed in the order they were registered
//! - **Single Poll Loop**: One event poll can handle multiple event types efficiently
//!
pub mod provider;
pub mod store;
pub mod v6;

use std::fmt;
// Re-export main types
pub use provider::{EventProvider, EventProviderError, EventProviderResult};

use anyhow::Error as AnyhowError;
use serde::{Deserialize, Serialize};
use serde_with::{BoolFromInt, serde_as};
use std::fmt::{Debug, Formatter};
use thiserror::Error;
pub use v6::EventSubscriberError;
pub use v6::EventSubscriberResult;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct EventId(String);

impl EventId {
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for EventId {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for EventId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Error)]
pub enum EventLoopError {
    #[error("Subscriber ({0}) failed to apply refresh event: {1}")]
    Refresh(String, Box<dyn EventSubscriberError>),
    #[error("Failed to read/write from/to store: {0}")]
    Store(AnyhowError),
    #[error("Failed to retrieve event: {0}")]
    Provider(Box<dyn EventProviderError>),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, Box<dyn EventSubscriberError>),
    #[error("Subscriber with `{0}` name already exists")]
    Register(&'static str),
    #[error("Failed to deserialize event: {0}")]
    Deserialize(AnyhowError),
    #[error("Cyclic dependency detected between event sources")]
    CyclicDependency,
    #[error("Event source {0} already registered")]
    DuplicateEventSource(&'static str),
    #[error("Failed to communicate with actor")]
    Actor,
}

impl From<Box<dyn EventProviderError>> for EventLoopError {
    fn from(err: Box<dyn EventProviderError>) -> Self {
        EventLoopError::Provider(err)
    }
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

    pub fn deserialize_generic<'a, T: Deserialize<'a>>(&'a self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.raw)
    }
}
impl RawEvent {
    fn event_id(&self) -> EventId {
        self.meta.event_id.clone().into_inner().into()
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

pub const MAX_ERROR_RETRIES: usize = 3;
