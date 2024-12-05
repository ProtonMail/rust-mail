//! Everything Proton Mailbox related.

pub mod actions;
pub mod context;
pub mod datatypes;
pub mod db;
pub mod errors;
mod events;
mod mailbox;
pub mod models;
pub mod sidebar;
mod user_context;

pub mod draft;
#[cfg(test)]
mod tests;

pub use context::{MailContext, MailContextError, MailContextResult};
pub use mailbox::{decrypted_message, DecryptedAttachment, Mailbox, MailboxError, MailboxResult};
pub use sidebar::{Sidebar, SidebarError, SidebarResult};
pub use user_context::{
    cache, MailUserContext, MailUserContextInitializationCallback, MailUserContextLoadingStage,
};

// re-exports
use crate::datatypes::LabelType;
use proton_api_core::service::ApiServiceError;
pub use proton_api_mail;
pub use proton_core_common;
use proton_core_common::cache::CacheError;
use proton_core_common::datatypes::{LabelId, LocalId, RemoteId};
use stash::stash::StashError;

use proton_action_queue::action::Id as ActionId;
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
        use stash::exports::ToSql;
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
    #[error("Attachment missing in database for local_id {0}")]
    ActionStillQueued(ActionId),
    #[error("Attachment missing in database for local_id {0}")]
    AttachmentMissing(LocalId),
    #[error("Unknown attachment with remote id {0}")]
    UnknownAttachment(RemoteId),
    #[error("Attachment {0} does not have a remote id")]
    AttachmentDoesNotHaveRemoteId(LocalId),
    #[error("Conversation with ID {0} is not in given view {1}")]
    ConversationDoesNotHaveLabel(LocalId, String),
    #[error("Conversation with ID {0} has no messages")]
    ConversationHasNoMessages(LocalId),
    #[error("Conversation with ID {0} has no remote ID")]
    ConversationHasNoRemoteId(LocalId),
    #[error("Conversation with ID {0} not found")]
    ConversationNotFound(LocalId),
    #[error("Empty list of conversations, expected at least one")]
    EmptyListOfConversations,
    #[error("Empty list of messages, expected at least one")]
    EmptyListOfMessages,
    #[error("Incorrect mime type: {0}")]
    InvalidMimeType(String),
    #[error("InvalidMobileActions: {0}")]
    InvalidMobileActions(String),
    #[error("Label with local id {0} does not have remote id")]
    LabelDoesNotHaveRemoteId(LocalId),
    #[error("Label with local id {0} not found")]
    LabelNotFound(LocalId),
    #[error("Local ID not found for {0} with remote ID {1}")]
    LocalIdNotFound(String, RemoteId),
    #[error("MessageBodyMetadata missing in database for message {0}")]
    MessageBodyMetadataMissing(LocalId),
    #[error("The cid {0} does not exist. The available ones are: [{1}]")]
    UnknownCid(String, String),
    #[error("Message with ID {0} has no remote ID")]
    MessageHasNoRemoteId(LocalId),
    #[error("Message missing in database for local_id {0}")]
    MessageMissing(LocalId),
    #[error("Message body missing for local_id {0}")]
    MessageBodyMissing(LocalId),
    #[error("Unknown Message with remote id {0}")]
    UnknownMessage(RemoteId),
    #[error("No conversation found in the current page which has a remote id")]
    NoConversationWithValidRemoteIdFoundInPage,
    #[error("No message found in the current page which has a remote id")]
    NoMessageWithValidRemoteIdFoundInPage,
    #[error("Remote ID not found for {0} with local ID {1}")]
    RemoteIdNotFound(String, LocalId),
    #[error("Could not find remote label {0}")]
    RemoteLabelDoesNotExist(LabelId),

    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),
    #[error("Can't deserialize from MessagePack: {0}")]
    RmpDeserialization(#[from] rmp_serde::decode::Error),
    #[error("Can't serialize into MessagePack: {0}")]
    RmpSerialization(#[from] rmp_serde::encode::Error),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
    #[error("Could not load user info")]
    UserNotFound,
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
