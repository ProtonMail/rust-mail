use crate::actions::ActionError;
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::Unexpected;
use crate::{draft::Error as DraftError, AppError, MailContextError, MailboxError, SidebarError};
use proton_action_queue::action::Action;
use proton_action_queue::queue::ActionError as InternalActionError;
use proton_api_core::service::ApiServiceError;
use proton_core_common::ContactError;
use proton_event_loop::subscriber::SubscriberError;
use proton_event_loop::EventLoopError;

/// TODO: Document this
#[derive(Debug)]
pub enum UserActionError {
    /// This error is related with the arguments (i.e. like a Message id who does not exist)
    InvalidAction(Reason),
    /// This error is related with the session (i.e. like a session expired)
    SessionExpired,
    /// This error come from the Backend (i.e. like a 404 error)
    ServerError(UserApiServiceError),
    /// This error come form network (i.e. like can't connect to backend)
    Network,
    /// Something unexpected happened
    Unexpected(Unexpected),
}

/// Reason for invalid Action
#[derive(Debug)]
pub enum Reason {
    InvalidParameter,
    UnknownLabel,
    UnknownMessage,
}

impl<E: Into<Unexpected>> From<E> for UserActionError {
    fn from(error: E) -> Self {
        Self::Unexpected(error.into())
    }
}

impl From<Reason> for UserActionError {
    fn from(reason: Reason) -> Self {
        Self::InvalidAction(reason)
    }
}

impl From<ApiServiceError> for UserActionError {
    fn from(error: ApiServiceError) -> Self {
        match UserApiServiceError::try_from(error) {
            Ok(api_service_error) => Self::ServerError(api_service_error),
            Err(unexpected) => Self::from(unexpected),
        }
    }
}

impl From<AppError> for UserActionError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::API(api_service_error) => Self::from(api_service_error),
            AppError::LabelDoesNotHaveRemoteId(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::LabelNotFound(_local_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::InvalidMimeType(_string) => Self::InvalidAction(Reason::InvalidParameter),
            AppError::MessageBodyMetadataMissing(_local_massage_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::RemoteLabelDoesNotExist(_label_id) => Self::Network,
            AppError::Cache(cache_error) => Self::from(cache_error),
            AppError::IO(io_error) => Self::from(io_error),
            AppError::Stash(stash_error) => Self::from(stash_error),
            AppError::Other(_string) => Self::Unexpected(Unexpected::Unknown),
            AppError::LocalIdNotFound(_string, _remote_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::RemoteIdNotFound(_string, _local_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::ActionStillQueued(_string) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentMissing(_string) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownAttachment(_) => Self::Unexpected(Unexpected::Unknown),
            AppError::AttachmentDoesNotHaveRemoteId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::ConversationDoesNotHaveLabel(_, _) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationNotFound(_) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationHasNoMessages(_) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationHasNoRemoteId(_local_id) => Self::Network,
            AppError::EmptyListOfConversations => Self::InvalidAction(Reason::InvalidParameter),
            AppError::EmptyListOfMessages => Self::InvalidAction(Reason::InvalidParameter),
            AppError::InvalidMobileActions(_) => Self::InvalidAction(Reason::InvalidParameter),
            AppError::MessageHasNoRemoteId(_local_id) => Self::Network,
            AppError::MessageMissing(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownMessage(_remote_id) => Self::Unexpected(Unexpected::Unknown),
            AppError::NoConversationWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::NoMessageWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::UserNotFound => Self::InvalidAction(Reason::InvalidParameter),
        }
    }
}

impl From<MailContextError> for UserActionError {
    fn from(error: MailContextError) -> Self {
        match error {
            MailContextError::Crypto
            | MailContextError::KeyChainHasNoKey
            | MailContextError::Login(_) => Self::Unexpected(Unexpected::Crypto),
            MailContextError::KeyChain(key_chain_error) => Self::from(key_chain_error),
            MailContextError::IO(io_error) => Self::from(io_error),
            MailContextError::DBMigration(migrator_error) => Self::from(migrator_error),
            MailContextError::EventLoop(event_loop_error) => Self::from(event_loop_error),
            MailContextError::ActionQueue(queue_error) => Self::from(queue_error),
            MailContextError::Action(action_error) => Self::from(action_error),
            MailContextError::QueuedAction(queued_error) => Self::from(queued_error),
            MailContextError::PGPKeyAccess(key_handling_error) => Self::from(key_handling_error),
            MailContextError::App(app_error) => Self::from(app_error),
            MailContextError::Stash(stash_error) => Self::from(stash_error),
            MailContextError::Api(api_service_error) => Self::from(api_service_error),
            MailContextError::CacheError(cache_error) => Self::from(cache_error),
            MailContextError::Other(anyhow) => Self::from(anyhow),
            MailContextError::ContactError(contact_error) => Self::from(contact_error),
            MailContextError::Draft(draft_error) => Self::from(draft_error),
        }
    }
}

impl From<DraftError> for UserActionError {
    fn from(_value: DraftError) -> Self {
        Self::Unexpected(Unexpected::Draft)
    }
}

impl From<EventLoopError> for UserActionError {
    fn from(_value: EventLoopError) -> Self {
        Self::Unexpected(Unexpected::Queue)
    }
}

impl From<SubscriberError> for UserActionError {
    fn from(_value: SubscriberError) -> Self {
        Self::Unexpected(Unexpected::Queue)
    }
}

impl From<ContactError> for UserActionError {
    fn from(error: ContactError) -> Self {
        match error {
            ContactError::CardNotFound(_string) => Self::InvalidAction(Reason::InvalidParameter),
            ContactError::ContactCardRemoteIdNotPresent(_string)
            | ContactError::FullContactNotFound(_string) => Self::Unexpected(Unexpected::Database),
            ContactError::Validation(_vcard_validation_error) => {
                Self::Unexpected(Unexpected::Unknown) // TODO: This will be changed in the future work on contacts
            }
        }
    }
}

impl From<ActionError> for UserActionError {
    fn from(error: ActionError) -> Self {
        match error {
            ActionError::Http(api_service_error) => Self::from(api_service_error),
            ActionError::Stash(stash_error) => Self::from(stash_error),
            ActionError::App(app_error) => Self::from(app_error),
            ActionError::NoInput => Self::Unexpected(Unexpected::Internal),
            ActionError::Other(anyhow) => Self::from(anyhow),
        }
    }
}

impl<T> From<InternalActionError<T>> for UserActionError
where
    T: Action,
    T::Error: Into<Self>,
{
    fn from(error: InternalActionError<T>) -> Self {
        match error {
            #[allow(clippy::useless_conversion)] // It is not useless clippy
            InternalActionError::Action(error) => Self::from(error.into()),
            InternalActionError::Queue(error) => Self::from(error),
        }
    }
}

impl From<MailboxError> for UserActionError {
    fn from(error: MailboxError) -> Self {
        match error {
            // Mailbox::new:     can't load Label from database
            // Mailbox::refresh: can't load Label from database
            // Mailbox::sync:    can't load Label from database
            MailboxError::LabelNotFound(_local_label_id) => {
                Self::InvalidAction(Reason::UnknownLabel)
            }
            // Mailbox::refresh: remote_id is None
            // Mailbox::sync:    remote_id is None
            MailboxError::LabelDoesNotHaveRemoteId(_local_label_id) => Self::Network,
            // Mailbox::sync_attachment: can't load Attachment from database
            MailboxError::AttachmentNotFound(_attachment_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            // Mailbox::decrypt_attachment: IO from std::io::copy
            MailboxError::AttachmentDecryptionIO(_string) => Self::Unexpected(Unexpected::Memory),
            // Mailbox::get_attachment_content: remote_id is None
            MailboxError::AttachmentDoesNotHaveRemoteId(_attachment_id) => Self::Network,

            MailboxError::APIError(api_service_error) => Self::from(api_service_error),
            MailboxError::AttachmentDecryption(attachment_decryption_error) => {
                Self::from(attachment_decryption_error)
            }
            MailboxError::AppError(app_error) => Self::from(app_error),
            MailboxError::Context(mail_context_error) => Self::from(mail_context_error),
            MailboxError::ActionQueue(queue_error) => Self::from(queue_error),
            MailboxError::InvalidAction(anyhow) => Self::from(anyhow),
            MailboxError::Stash(stash_error) => Self::from(stash_error),
            MailboxError::MessageDecryption(message_error) => Self::from(message_error),
            MailboxError::Cache(cache_error) => Self::from(cache_error),
            MailboxError::IO(io_error) => Self::from(io_error),

            // Not currently used
            MailboxError::NoExclusiveLocationFound(_) => todo!(),
            MailboxError::ConversationNotFound(_) => todo!(),
            MailboxError::ConversationDoesNotHaveRemoteId(_) => todo!(),
            MailboxError::MessageDoesNotHaveRemoteId(_) => todo!(),
            MailboxError::MessageNotFound(_) => todo!(),
            MailboxError::ConversationError(_) => todo!(),
            MailboxError::ConversationHasNoMessages(_) => todo!(),
            MailboxError::InvalidViewMode => todo!(),
        }
    }
}

impl From<SidebarError> for UserActionError {
    fn from(error: SidebarError) -> Self {
        match error {
            SidebarError::MailContext(mail_context_error) => Self::from(mail_context_error),
            SidebarError::Stash(stash_error) => Self::from(stash_error),
            SidebarError::AppError(app_error) => Self::from(app_error),
        }
    }
}
