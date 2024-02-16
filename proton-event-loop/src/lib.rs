mod background_loop;
#[cfg(test)]
mod loop_tests;
mod provider;
mod store;
mod subscriber;

mod foreground_loop;
#[cfg(feature = "uniffi")]
pub mod uniffi_bindings;

pub use background_loop::*;
pub use foreground_loop::*;
pub use proton_async;
pub use provider::*;
pub use store::*;
pub use subscriber::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub use paste;

use proton_api_core::exports::{anyhow, thiserror};
use proton_api_core::http::HttpRequestError;

#[derive(Debug, thiserror::Error)]
pub enum EventLoopError {
    #[error("Failed to read from store: {0}")]
    StoreRead(anyhow::Error),
    #[error("Failed to write store: {0}")]
    StoreWrite(anyhow::Error),
    #[error("Failed to retrieve event: {0}")]
    Provider(#[from] HttpRequestError),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, SubscriberError),
    #[error("Other: {0}")]
    Other(String),
}
