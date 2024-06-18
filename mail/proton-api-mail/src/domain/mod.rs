//! Domain Types.

mod attachments;
mod conversations;
mod event;
mod image_proxy;
mod labels;
mod messages;
mod settings;

pub use attachments::*;
pub use conversations::*;
pub use event::*;
pub use image_proxy::*;
pub use labels::*;
pub use messages::*;
use proton_api_core::http::RequestError;
pub use settings::*;
use stash::stash::StashError;

/// Errors that may occur while interacting with the API. This is temporary.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP Error: {0}")]
    Http(#[from] RequestError),
    #[error("Stash Error: {0}")]
    Stash(#[from] StashError),
    #[error("Other Error: {0}")]
    Other(String),
}
