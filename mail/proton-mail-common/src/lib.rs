//! Everything Proton Mailbox related.

pub mod actions;
mod context;
pub mod db;
mod mailbox;

pub mod avatar;

pub mod datatypes;
mod events;
pub mod models;
mod proton_color;
pub mod sidebar;
mod user_context;

#[cfg(test)]
mod tests;

pub use context::*;
pub use mailbox::*;
pub use sidebar::*;
pub use user_context::*;

// re-exports
use crate::datatypes::LabelType;
use proton_api_core::service::ApiServiceError;
pub use proton_api_mail;
pub use proton_core_common;
use proton_core_common::cache::CacheError;
use proton_core_common::datatypes::{LabelId, LocalId, RemoteId};
use stash::stash::StashError;

use thiserror::Error;

pub const ALL_LABEL_TYPES: [LabelType; 4] = [
    LabelType::Label,
    LabelType::ContactGroup,
    LabelType::Folder,
    LabelType::System,
];

/// Errors that may occur while using the ProtonMail app.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
    #[error("Label with local id {0} does not have remote id")]
    LabelDoesNotHaveRemoteId(LocalId),
    #[error("Label with local id {0} not found")]
    LabelNotFound(LocalId),
    #[error("Local ID not found for {0} with remote ID {1}")]
    LocalIdNotFound(String, RemoteId),
    #[error("Incorrect mime type: {0}")]
    InvalidMimeType(String),
    #[error("MessageBodyMetadata missing in database for message {0}")]
    MessageBodyMetadataMissing(LocalId),
    #[error("Could not find remote label {0}")]
    RemoteLabelDoesNotExist(LabelId),
    #[error("Remote ID not found for {0} with local ID {1}")]
    RemoteIdNotFound(String, LocalId),
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
    #[error("Other error: {0}")]
    Other(String),
}
