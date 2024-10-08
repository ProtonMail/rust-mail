//! Utilities to listen to the proton event loop. This crate provides both a Foreground event loop
//! ([`EventLoop`]) and a Background event loop ([`BackgroundEventLoop`]). Handling of events is
//! delegated to a [`Subscriber`]. These need to be registered with either loop version.
//!
//! # Foreground Example
//! This version of the loop requires the user to poll the loop manually so that it can progress.
//! ```ignore
//! use proton_api_core::domain::Event;
//! use proton_event_loop::{EventLoop, Provider, Store};
//!
//! async fn create_loop_and_poll<T:Event>(store:Box<dyn Store>, provider:Box<dyn Provider<T>>) {
//!     let mut event_loop = EventLoop::new();
//!
//!     loop {
//!         if let Err(_) = event_loop.poll(store.as_ref(), provider.as_ref(),&[]).await {
//!             //Handle error
//!         }
//!     }
//! }
//!
//! ```
//!
//! # Background Example
//! This version of the loop runs automatically in a background task with a user defined interval.
//! Additionally, this version also has modifiers to pause and resume the loop.
//! ```ignore
//! use std::time::Duration;
//! use proton_api_core::domain::Event;
//! use proton_event_loop::{BackgroundEventLoop, EventLoop, EventLoopErrorHandler, Provider, Store};
//!
//! async fn create_background_loop<T:Event+'static>(store:Box<dyn Store>, provider:Box<dyn Provider<T>>, error_handler:Box<dyn EventLoopErrorHandler>) {
//!     let bg_event_loop = BackgroundEventLoop::new();
//!
//!     bg_event_loop.start(Duration::from_secs(15), store, provider, error_handler).await.unwrap();
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
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId;
use proton_api_core::services::proton::responses::GetEventResponse;
use serde::Deserialize;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventLoopError {
    #[error("Failed to read from store: {0}")]
    StoreRead(AnyhowError),
    #[error("Failed to write store: {0}")]
    StoreWrite(AnyhowError),
    #[error("Failed to retrieve event: {0}")]
    Provider(#[from] ApiServiceError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
    #[error("Other: {0}")]
    Other(String),
}

/// TODO: Document this trait.
pub trait Event:
    Clone
    + Debug
    // + for<'de> Deserialize<'de>
    + Eq
    + PartialEq
    // + Serialize
    + Send
    + Sync
    + 'static
{
    /// The type of remote ID used by the code that creates and handles the
    /// events. Note that this will most likely be the `RemoteId` type of the
    /// crate in question, which cannot be specified in this crate, hence is
    /// left to be defined.
    type Id: Clone + Debug + Display + Eq + From<RemoteId> + Hash + Into<RemoteId> + PartialEq + Send + Sync;

    /// The API response type of the event.
    type Response: GetEventResponse + Clone + Debug + for<'de> Deserialize<'de> + Eq + PartialEq + Send + Sync;

    /// Get the event id of the event.
    fn event_id(&self) -> &Self::Id;

    /// Check if the event has more data.
    fn has_more(&self) -> bool;
}
