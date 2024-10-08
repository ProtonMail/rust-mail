use crate::actions::ActionError;
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::Unexpected;
use crate::{AppError, MailContextError};
use proton_api_core::login::LoginError;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::response_data::HumanVerificationChallenge;
use proton_core_common::ContactError;
use proton_event_loop::subscriber::SubscriberError;
use proton_event_loop::EventLoopError;

/// User Localizable Error for Login flow
#[derive(Debug)]
pub enum UserLoginFlowError {
    /// This error is related with the arguments (i.e. like a Message id who does not exist)
    Reason(Reason),
    /// This error come from the Backend (i.e. like a 404 error)
    ServerError(UserApiServiceError),
    /// This error come form network (i.e. like can't connect to backend)
    Network,
    /// Something unexpected happened
    Unexpected(Unexpected),
}

/// Reason specific for this error
#[derive(Debug)]
pub enum Reason {
    HumanVerificationChallenge(HumanVerificationChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

impl<E: Into<Unexpected>> From<E> for UserLoginFlowError {
    fn from(error: E) -> Self {
        Self::Unexpected(error.into())
    }
}

impl From<Reason> for UserLoginFlowError {
    fn from(reason: Reason) -> Self {
        Self::Reason(reason)
    }
}

impl From<ApiServiceError> for UserLoginFlowError {
    fn from(error: ApiServiceError) -> Self {
        match UserApiServiceError::try_from(error) {
            Ok(api_service_error) => Self::ServerError(api_service_error),
            Err(unexpected) => Self::from(unexpected),
        }
    }
}

impl From<LoginError> for UserLoginFlowError {
    fn from(error: LoginError) -> Self {
        match error {
            LoginError::HumanVerificationRequired(human_verification_challenge) => Self::Reason(
                Reason::HumanVerificationChallenge(human_verification_challenge),
            ),
            LoginError::InvalidState => Self::Unexpected(Unexpected::Internal),
            LoginError::KeySecretAuthUpdate(_)
            | LoginError::KeySecretDecryption
            | LoginError::KeySecretDerivation(_) => Self::Reason(Reason::CantUnlockUserKey),
            LoginError::KeySecretSaltFetch(api_service_error) => match api_service_error {
                // HTTP code 422
                ApiServiceError::UnprocessableEntity(_string1, _string2) => {
                    // TODO(ET-1076): use api_code: 8002 -> InvalidCredentials ; 2005 -> EmptyInput ; other -> Self::from(api_service_error)
                    Self::Reason(Reason::InvalidCredentials)
                }
                _ => Self::from(api_service_error),
            },
            LoginError::ServerProof(_string) | LoginError::SrpProof(_string) => {
                Self::Reason(Reason::InvalidCredentials)
            }
            LoginError::UnsupportedTfa => Self::Reason(Reason::UnsupportedTfa),
            LoginError::WrongMailboxPassword => Self::Unexpected(Unexpected::Internal),
            LoginError::AuthStore(store_error) => Self::from(store_error),
        }
    }
}

impl From<MailContextError> for UserLoginFlowError {
    fn from(error: MailContextError) -> Self {
        match error {
            MailContextError::Crypto | MailContextError::KeyChainHasNoKey => {
                Self::Unexpected(Unexpected::Crypto)
            }
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
            MailContextError::Login(login_error) => Self::from(login_error),
            MailContextError::Api(api_service_error) => Self::from(api_service_error),
            MailContextError::CacheError(cache_error) => Self::from(cache_error),
            MailContextError::Other(anyhow) => Self::from(anyhow),
            MailContextError::ContactError(contact_error) => Self::from(contact_error),
        }
    }
}

impl From<EventLoopError> for UserLoginFlowError {
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

impl From<ActionError> for UserLoginFlowError {
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

impl From<AppError> for UserLoginFlowError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::API(api_service_error) => Self::from(api_service_error),
            AppError::LabelDoesNotHaveRemoteId(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::LabelNotFound(_local_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::LocalIdNotFound(_string, _remote_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::InvalidMimeType(_string) => Self::Unexpected(Unexpected::Unknown),
            AppError::MessageBodyMetadataMissing(_local_massage_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::RemoteLabelDoesNotExist(_label_id) => Self::Network,
            AppError::RemoteIdNotFound(_string, _local_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::ConversationNotFound(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationHasNoRemoteId(_) | AppError::MessageHasNoRemoteId(_) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::ConversationHasNoMessages(_local_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::Cache(cache_error) => Self::from(cache_error),
            AppError::IO(io_error) => Self::from(io_error),
            AppError::Stash(stash_error) => Self::from(stash_error),
            AppError::Other(_string) => Self::Unexpected(Unexpected::Unknown),
            AppError::AttachmentMissing(_)
            | AppError::MessageMissing(_)
            | AppError::EmptyListOfConversations
            | AppError::EmptyListOfMessages
            | AppError::NoConversationWithValidRemoteIdFoundInPage
            | AppError::NoMessageWithValidRemoteIdFoundInPage
            | AppError::ConversationDoesNotHaveLabel(_, _) => {
                Self::Unexpected(Unexpected::Internal)
            }
        }
    }
}

impl From<ContactError> for UserLoginFlowError {
    fn from(error: ContactError) -> Self {
        match error {
            ContactError::CardNotFound(_string) => Self::Unexpected(Unexpected::Internal),
            ContactError::ContactCardRemoteIdNotPresent(_string)
            | ContactError::FullContactNotFound(_string) => Self::Unexpected(Unexpected::Database),
            _ => todo!(),
        }
    }
}

impl From<SubscriberError> for UserLoginFlowError {
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
