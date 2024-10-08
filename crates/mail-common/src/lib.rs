//! Everything Proton Mailbox related.

pub mod actions;
pub mod avatar;
mod context;
pub mod datatypes;
pub mod db;
pub mod errors;
mod events;
mod mailbox;
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

#[macro_export]
macro_rules! find_in_query {
    ($query:expr, $params:expr) => {{
        let params = $params
            .into_iter()
            .map(|param| Box::new(param) as Box<dyn ToSql + Send>)
            .collect::<Vec<_>>();
        let query = format!($query, vec!["?"; params.len()].join(","),);
        (query, params)
    }};
}

/// Errors that may occur while using the ProtonMail app.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
    #[error("Attachment missing in database for local_id {0}")]
    AttachmentMissing(LocalId),
    #[error("Conversation with ID {0} not found")]
    ConversationNotFound(LocalId),
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
    #[error("Message missing in database for local_id {0}")]
    MessageMissing(LocalId),
    #[error("Could not find remote label {0}")]
    RemoteLabelDoesNotExist(LabelId),
    #[error("Remote ID not found for {0} with local ID {1}")]
    RemoteIdNotFound(String, LocalId),
    #[error("Conversation with ID {0} has no remote ID")]
    ConversationHasNoRemoteId(LocalId),
    #[error("Message with ID {0} has no remote ID")]
    MessageHasNoRemoteId(LocalId),
    #[error("Conversation with ID {0} has no messages")]
    ConversationHasNoMessages(LocalId),
    #[error("Empty list of conversations, expected at least one")]
    EmptyListOfConversations,
    #[error("Empty list of messages, expected at least one")]
    EmptyListOfMessages,
    #[error("Conversation with ID {0} is not in given view {1}")]
    ConversationDoesNotHaveLabel(LocalId, String),
    #[error("No conversation found in the current page which has a remote id")]
    NoConversationWithValidRemoteIdFoundInPage,
    #[error("No message found in the current page which has a remote id")]
    NoMessageWithValidRemoteIdFoundInPage,
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
