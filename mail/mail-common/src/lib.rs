//! Everything Proton Mailbox related.

pub mod actions;
pub mod context;
pub mod datatypes;
pub mod db;
pub mod errors;
mod events;
mod mailbox;
pub mod models;
pub mod prefetch;
pub mod sidebar;
mod user_context;

pub mod background_execution;
pub mod draft;
pub mod mail_scroller;
mod send_queries;
#[cfg(test)]
mod tests;

pub use context::{MailContext, MailContextError, MailContextResult};
pub use mailbox::{DecryptedAttachment, Mailbox, MailboxError, MailboxResult, decrypted_message};
use proton_core_common::models::LabelError;
pub use sidebar::{Sidebar, SidebarError, SidebarResult};
pub use user_context::MailUserContext;

// re-exports
use crate::datatypes::{LocalAttachmentId, LocalMessageId};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::LabelId;
pub use proton_core_common;
use proton_core_common::datatypes::{LocalAddressId, LocalLabelId};
pub use proton_mail_api;
use stash::stash::StashError;

use datatypes::attachment::ContentId;
use proton_action_queue::action::ActionId;
use proton_mail_api::services::proton::common::{AttachmentId, MessageId};
use proton_mail_ids::LocalConversationId;
use thiserror::Error;

// Avoid breaking back compat.
//
// TODO: We should probably use a better name at some point for the clients like "protonSdk" or something
// but that would be a breaking change
// (fixed with search and replace but something we need to coordinate.)
#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[macro_export]
macro_rules! find_in_query {
    ($query:expr, $params:expr) => {{
        use stash::exports::ToSql;
        let params = $params
            .into_iter()
            .map(|param| Box::new(param) as Box<dyn ToSql + Send>)
            .collect::<Vec<_>>();
        let query = format!($query, ::stash::utils::placeholders(params.len()),);
        (query, params)
    }};
}

/// Errors that may occur while using the ProtonMail app.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Attachment missing in database for local_id {0}")]
    ActionStillQueued(ActionId),
    #[error("Attachment missing in database for local_id {0}")]
    AttachmentMissing(LocalAttachmentId),
    #[error("Unknown attachment with remote id {0}")]
    UnknownAttachment(AttachmentId),
    #[error("Attachment {0} does not have a remote id")]
    AttachmentDoesNotHaveRemoteId(LocalAttachmentId),
    #[error("Conversation with ID {0} is not in given view {1}")]
    ConversationDoesNotHaveLabel(LocalConversationId, String),
    #[error("Conversation with ID {0} has no messages")]
    ConversationHasNoMessages(LocalConversationId),
    #[error("Conversation with ID {0} has no remote ID")]
    ConversationHasNoRemoteId(LocalConversationId),
    #[error("Conversation with ID {0} not found")]
    ConversationNotFound(LocalConversationId),
    #[error("Empty list of conversations, expected at least one")]
    EmptyListOfConversations,
    #[error("Empty list of messages, expected at least one")]
    EmptyListOfMessages,
    #[error("Incorrect mime type: {0}")]
    InvalidMimeType(String),
    #[error("InvalidMobileActions: {0}")]
    InvalidMobileActions(String),
    #[error("Label with local id {0} does not have remote id")]
    LabelDoesNotHaveRemoteId(LocalLabelId),
    #[error("Label with local id {0} not found")]
    LabelNotFound(LocalLabelId),
    #[error("Local ID not found for {0} with remote ID {1}")]
    LocalIdNotFound(String, String),
    #[error("MessageBodyMetadata missing in database for message {0}")]
    MessageBodyMetadataMissing(LocalMessageId),
    #[error("The cid {0} does not exist. The available ones are: {1:#?}")]
    UnknownCid(ContentId, Vec<ContentId>),
    #[error("Message with ID {0} has no remote ID")]
    MessageHasNoRemoteId(LocalMessageId),
    #[error("Message missing in database for local_id {0}")]
    MessageMissing(LocalMessageId),
    #[error("Message body missing for local_id {0}")]
    MessageBodyMissing(LocalMessageId),
    #[error("Unknown Message with remote id {0}")]
    UnknownMessage(MessageId),
    #[error("No conversation found in the current page which has a remote id")]
    NoConversationWithValidRemoteIdFoundInPage,
    #[error("No message found in the current page which has a remote id")]
    NoMessageWithValidRemoteIdFoundInPage,
    #[error("Address {0} does not have a remote id")]
    AddressHasNoRemoteId(LocalAddressId),
    #[error("Could not find remote label {0}")]
    RemoteLabelDoesNotExist(LabelId),
    #[error("Could not find counters for remote label {0}")]
    RemoteLabelHasNoCounters(LabelId),
    #[error("Could not find counters for local label {0}")]
    LocalLabelHasNoCounters(LocalLabelId),
    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
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
    #[error("Label error: {0}")]
    Label(#[from] LabelError),
    #[error("Attachment {0} has no address id")]
    AttachmentHasNoAddressId(LocalAttachmentId),
    #[error("Attachment {0} does not have key packets")]
    AttachmentMissingKeyPackets(LocalAttachmentId),
    #[error("Attachment {0} is not in the cache")]
    AttachmentIsNotInCache(LocalAttachmentId),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
