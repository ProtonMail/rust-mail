#![allow(clippy::result_large_err)] // TODO(ET-5588): address growing Error size

pub mod actions;
mod background_execution;
mod context;
pub mod datatypes;
pub mod db;
pub mod draft;
mod errors;
mod image_loader;
mod ios_share_ext;
#[allow(clippy::result_large_err)]
mod mail_cursor;
#[allow(clippy::result_large_err)]
mod mail_scroller;
mod mailbox;
mod migration_snooper;
pub mod models;
mod prefetch;
mod rsvp;
mod send_queries;
mod sidebar;
mod snooze;
mod tracker_service;
mod traits;
mod upsell_eligibility_watcher;
mod user_context;

#[cfg(feature = "test-utils")]
pub mod test_utils;

pub use self::background_execution::*;
pub use self::context::*;
pub use self::errors::*;
pub use self::image_loader::*;
pub use self::ios_share_ext::*;
pub use self::mail_cursor::*;
pub use self::mail_scroller::*;
pub use self::mailbox::*;
pub use self::migration_snooper::*;
pub use self::rsvp::*;
pub use self::sidebar::*;
pub use self::snooze::*;
pub use self::tracker_service::*;
pub use self::traits::*;
pub use self::upsell_eligibility_watcher::*;
pub use self::user_context::*;
use crate::datatypes::{LocalAttachmentId, LocalConversationId, LocalMessageId};
use datatypes::attachment::ContentId;
use proton_action_queue::action::Action;
use proton_action_queue::queue::{ActionError, MultiActionError};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LocalAddressId, LocalLabelId};
use proton_core_common::models::LabelError;
use proton_crypto_inbox::attachment::AttachmentDecryptionError;
use proton_mail_api::services::proton::common::ConversationId;
use stash::stash::StashError;
use thiserror::Error;

#[macro_export]
macro_rules! find_in_query {
    ($query:expr, $params:expr) => {{
        use stash::exports::ToSql;
        let params = $params
            .into_iter()
            .map(|param| Box::new(param) as Box<dyn ToSql + Send>)
            .collect::<Vec<_>>();
        let query = format!($query, ::stash::utils::placeholders(&params),);
        (query, params)
    }};
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Snooze time is in the past")]
    SnoozeTimeInThePast,
    #[error("Invalid snooze location: {0}")]
    InvalidSnoozeLocation(String),
    #[error("Could not calculate snooze options")]
    CouldNotCalculateSnoozeOptions,
    #[error("Attachment missing in database for local_id {0}")]
    AttachmentMissing(LocalAttachmentId),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryption(#[from] AttachmentDecryptionError),
    #[error("Attachment decryption failed: {0}")]
    AttachmentDecryptionIO(String),
    #[error("Conversation with ID {0} is not in given view {1}")]
    ConversationDoesNotHaveLabel(LocalConversationId, String),
    #[error("Conversation with ID {0} has no remote ID")]
    ConversationHasNoRemoteId(LocalConversationId),
    #[error("Conversation with ID {0} not found")]
    ConversationNotFound(LocalConversationId),
    #[error("Conversation with ID {0} does not exist on server")]
    ConversationDoesNotExistOnServer(ConversationId),
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
    #[error("The cid {0} does not exist. The available ones are: {1:#?}")]
    UnknownCid(ContentId, Vec<ContentId>),
    #[error("Message with ID {0} has no remote ID")]
    MessageHasNoRemoteId(LocalMessageId),
    #[error("Message missing in database for local_id {0}")]
    MessageMissing(LocalMessageId),
    #[error("Address missing in database for local_id {0}")]
    AddressMissing(LocalAddressId),
    #[error("Address {0} does not have a remote id")]
    AddressHasNoRemoteId(LocalAddressId),
    #[error("Could not find remote label {0}")]
    RemoteLabelDoesNotExist(LabelId),
    #[error("Could not find counters for local label {0}")]
    LocalLabelHasNoCounters(LocalLabelId),
    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Stash error: {0}")]
    Stash(#[from] StashError),
    #[error("Label error: {0}")]
    Label(#[from] LabelError),
    #[error("Attachment with ID {0} not found")]
    AttachmentHasNoRemoteId(LocalAttachmentId),
    #[error("Attachment {0} has no address id")]
    AttachmentHasNoAddressId(LocalAttachmentId),
    #[error("Attachment {0} does not have key packets")]
    AttachmentMissingKeyPackets(LocalAttachmentId),
    #[error("Attachment {0} is not in the cache")]
    AttachmentIsNotInCache(LocalAttachmentId),
    #[error("{0}")]
    ActionError(#[from] MultiActionError),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl<T: Action> From<ActionError<T>> for AppError {
    fn from(value: ActionError<T>) -> Self {
        Self::ActionError(value.into())
    }
}
