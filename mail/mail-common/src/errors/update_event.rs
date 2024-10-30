use super::api_service_error::UserApiServiceError;
use crate::actions::ActionError;
use crate::draft::Error as DraftError;
use crate::errors::unexpected::Unexpected;
use crate::{AppError, MailContextError, MailboxError, SidebarError};
use proton_api_core::service::ApiServiceError;
use proton_core_common::ContactError;
use proton_event_loop::subscriber::SubscriberError;
use proton_event_loop::EventLoopError;

#[derive(Debug)]
pub enum UpdateEventError {
    /// This error is related with the arguments (i.e. like a Message id who does not exist)
    Reason(Reason),
    /// This error is related with the session (i.e. like a session expired)
    SessionExpired,
    /// This error come from the Backend (i.e. like a 404 error)
    ServerError(UserApiServiceError),
    /// This error come form network (i.e. like can't connect to backend)
    Network,
    /// Something unexpected happened
    Unexpected(Unexpected),
}

impl<E: Into<Unexpected>> From<E> for UpdateEventError {
    fn from(error: E) -> Self {
        Self::Unexpected(error.into())
    }
}

/// Reason for invalid Action
#[derive(Debug)]
pub enum Reason {
    UnknownLabel,
}

impl From<Reason> for UpdateEventError {
    fn from(reason: Reason) -> Self {
        Self::Reason(reason)
    }
}

impl From<ApiServiceError> for UpdateEventError {
    fn from(error: ApiServiceError) -> Self {
        match UserApiServiceError::try_from(error) {
            Ok(api_service_error) => Self::ServerError(api_service_error),
            Err(unexpected) => Self::from(unexpected),
        }
    }
}

impl From<EventLoopError> for UpdateEventError {
    fn from(error: EventLoopError) -> Self {
        match error {
            EventLoopError::StoreRead(anyhow) | EventLoopError::StoreWrite(anyhow) => {
                Self::from(anyhow)
            }
            EventLoopError::Provider(api_service_error) => Self::from(api_service_error),
            EventLoopError::Subscriber(_string, subscriber_error) => Self::from(subscriber_error),
            EventLoopError::Other(_string) => Self::Unexpected(Unexpected::Unknown),
        }
    }
}

impl From<SubscriberError> for UpdateEventError {
    fn from(error: SubscriberError) -> Self {
        match error {
            SubscriberError::Api(api_service_error) => Self::from(api_service_error),
            SubscriberError::Other(anyhow) => Self::from(anyhow),
            SubscriberError::Send | SubscriberError::Receive => {
                Self::Unexpected(Unexpected::Internal)
            }
            SubscriberError::StashError(stash_error) => Self::from(stash_error),
        }
    }
}

impl From<MailContextError> for UpdateEventError {
    fn from(error: MailContextError) -> Self {
        match error {
            MailContextError::Crypto | MailContextError::KeyChainHasNoKey => {
                Self::Unexpected(Unexpected::Crypto)
            }
            MailContextError::KeyChain(key_chain_error) => Self::from(key_chain_error),
            MailContextError::IO(io_error) => Self::from(io_error),
            MailContextError::DBMigration(migrator_error) => Self::from(migrator_error),
            MailContextError::EventLoop(event_loop_error) => Self::from(event_loop_error),
            MailContextError::QueuedAction(queued_error) => Self::from(queued_error),
            MailContextError::PGPKeyAccess(key_handling_error) => Self::from(key_handling_error),
            MailContextError::App(app_error) => Self::from(app_error),
            MailContextError::Stash(stash_error) => Self::from(stash_error),
            MailContextError::Api(api_service_error) => Self::from(api_service_error),
            MailContextError::CacheError(cache_error) => Self::from(cache_error),
            MailContextError::ContactError(contact_error) => Self::from(contact_error),
            _ => Self::Unexpected(Unexpected::Unknown),
        }
    }
}

impl From<DraftError> for UpdateEventError {
    fn from(_error: DraftError) -> Self {
        Self::Unexpected(Unexpected::Draft)
    }
}

impl From<ActionError> for UpdateEventError {
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

impl From<AppError> for UpdateEventError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::API(api_service_error) => Self::from(api_service_error),
            AppError::LabelDoesNotHaveRemoteId(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::LabelNotFound(_local_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::InvalidMimeType(_string) => Self::Unexpected(Unexpected::Internal),
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
            AppError::ActionStillQueued(_id) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentMissing(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownAttachment(_remote_id) => Self::Unexpected(Unexpected::Database),
            AppError::AttachmentDoesNotHaveRemoteId(_local_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::ConversationDoesNotHaveLabel(_local_id, _) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::ConversationHasNoMessages(_local_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::ConversationHasNoRemoteId(_local_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::ConversationNotFound(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::EmptyListOfConversations => Self::Unexpected(Unexpected::Database),
            AppError::EmptyListOfMessages => Self::Unexpected(Unexpected::Database),
            AppError::InvalidMobileActions(_) => Self::Unexpected(Unexpected::InvalidArgument),
            AppError::MessageHasNoRemoteId(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::MessageMissing(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownMessage(_remote_id) => Self::Unexpected(Unexpected::Database),
            AppError::NoConversationWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::NoMessageWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::UserNotFound => Self::Unexpected(Unexpected::Unknown),
        }
    }
}

impl From<ContactError> for UpdateEventError {
    fn from(error: ContactError) -> Self {
        match error {
            ContactError::CardNotFound(_string) => Self::Unexpected(Unexpected::Unknown),
            ContactError::ContactCardRemoteIdNotPresent(_string)
            | ContactError::FullContactNotFound(_string) => Self::Unexpected(Unexpected::Database),
            ContactError::Validation(_) => Self::Unexpected(Unexpected::InvalidArgument),
        }
    }
}

impl From<MailboxError> for UpdateEventError {
    fn from(error: MailboxError) -> Self {
        match error {
            MailboxError::LabelNotFound(_local_label_id) => Self::Reason(Reason::UnknownLabel),
            MailboxError::LabelDoesNotHaveRemoteId(_local_label_id) => Self::Network,
            MailboxError::AttachmentNotFound(_attachment_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            MailboxError::AttachmentDecryptionIO(_string) => Self::Unexpected(Unexpected::Memory),
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

impl From<SidebarError> for UpdateEventError {
    fn from(error: SidebarError) -> Self {
        match error {
            SidebarError::MailContext(mail_context_error) => Self::from(mail_context_error),
            SidebarError::Stash(stash_error) => Self::from(stash_error),
            SidebarError::AppError(app_error) => Self::from(app_error),
        }
    }
}
