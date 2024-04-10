//! Utilities to listen to the proton event loop. This crate provides both a Foreground event loop
//! ([`EventLoop`]) and a Background event loop ([`BackgroundEventLoop`]). Handling of events is
//! delegated to a [`Subscriber`]. These need to be registered with either loop version.
//!
//! # Foreground Example
//! This version of the loop requires the user to poll the loop manually so that it can progress.
//! ```
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
//! ```
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
mod background_loop;
#[cfg(test)]
mod loop_tests;
mod provider;
mod store;
mod subscriber;

mod foreground_loop;

pub use background_loop::*;
pub use foreground_loop::*;
pub use paste;
pub use proton_async;
pub use provider::*;
pub use store::*;
pub use subscriber::*;

use proton_api_core::exports::{anyhow, thiserror};
use proton_api_core::http::RequestError;

#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
    #[error("Failed to read from store: {0}")]
    StoreRead(anyhow::Error),
    #[error("Failed to write store: {0}")]
    StoreWrite(anyhow::Error),
    #[error("Failed to retrieve event: {0}")]
    Provider(#[from] RequestError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
    #[error("Other: {0}")]
    Other(String),
}
