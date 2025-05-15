//! Utilities to listen to the proton event loop. This crate provides both a Foreground event loop
//! ([`EventLoop`]) and a Background event loop ([`BackgroundEventLoop`]).
//! Handling of events is delegated to a [`Subscriber`]. These need to be registered with either loop version.
//!
//! # Foreground Event Loop
//!
//! This version of the loop requires the user to poll the loop manually so that it can progress.
//! The user is fully responsible for handling errors at the poll call site.
//! This is also the only one we currently use.
//!
//! ## Example
//!
//! ```ignore
//! use proton_core_api::domain::Event;
//! use proton_event_loop::{EventLoop, Provider, Store};
//!
//! async fn create_loop_and_poll<T: Event>(store: &dyn Store, provider: &dyn Provider<T>) {
//!     let mut event_loop = EventLoop::new();
//!
//!     loop {
//!         if let Err(_) = event_loop.poll(store, provider, &[]).await {
//!             // Handle error
//!         }
//!     }
//! }
//! ```
//!
//! # Background Event Loop
//!
//! This version of the loop runs automatically in a background task with a user defined interval.
//! Additionally, this version also has modifiers to pause, resume and cancel the loop.
//! You need to provide a custom error handler to it.
//! This is currently not used.
//!
//! ## Example
//!
//! ```ignore
//! use std::time::Duration;
//! use proton_core_api::domain::Event;
//! use proton_event_loop::{BackgroundEventLoop, EventLoop, EventLoopErrorHandler, Provider, Store};
//!
//! async fn create_background_loop<Ev: Event + 'static>(
//!     store: Box<dyn Store>,
//!     provider: Box<dyn Provider<Ev>>,
//!     error_handler: Box<dyn EventLoopErrorHandler>,
//! ) {
//!     let bg_event_loop = BackgroundEventLoop::new();
//!
//!     bg_event_loop
//!         .start(Duration::from_secs(15), store, provider, error_handler)
//!         .await
//!         .unwrap();
//!     // Background event loop is always created in a paused state
//!     bg_event_loop.resume();
//!
//!     // Events are now processed in the background.
//! }
//!
//! ```
//!
pub mod background_loop;
pub mod foreground_loop;
pub mod provider;
pub mod store;
pub mod subscriber;

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;

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
    #[error("We were asked to refresh, but this is not implemented")]
    Refresh,
    #[error("Failed to read from store: {0}")]
    StoreRead(AnyhowError),
    #[error("Failed to write store: {0}")]
    StoreWrite(AnyhowError),
    #[error("Failed to retrieve event: {0}")]
    Provider(#[from] ApiServiceError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
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

#[serde_as]
#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RawEvent {
    #[serde(rename = "EventID")]
    event_id: EventId,
    #[serde(rename = "More")]
    #[serde_as(as = "BoolFromInt")]
    has_more: bool,
    refresh: u8,
    #[serde(skip)]
    raw: Vec<u8>,
}

impl RawEvent {
    pub fn from_json_bytes(raw: Vec<u8>) -> Result<Self, AnyhowError> {
        let json = std::str::from_utf8(&raw)?;
        let mut this: Self = serde_json::from_str(json)?;
        this.raw = raw;

        Ok(this)
    }

    pub fn deserialize<T: Event + From<<T as Event>::Response>>(&self) -> Result<T, AnyhowError> {
        let json = std::str::from_utf8(&self.raw)?;
        let event = T::from(serde_json::from_str(json)?);

        Ok(event)
    }
}

impl Event for RawEvent {
    type Response = Self;

    fn event_id(&self) -> &EventId {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }

    fn is_refresh(&self) -> bool {
        self.refresh != 0
    }
}
