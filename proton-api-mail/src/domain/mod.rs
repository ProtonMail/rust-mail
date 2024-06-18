//! Domain Types.

mod attachments;
mod conversations;
mod event;
mod image_proxy;
mod labels;
mod messages;
mod settings;

use proton_api_core::http::RequestError;
use stash::stash::StashError;
pub use attachments::*;
pub use conversations::*;
pub use event::*;
pub use image_proxy::*;
pub use labels::*;
pub use messages::*;
pub use settings::*;

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
