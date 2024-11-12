use crate::actions::ActionError;
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::Unexpected;
use crate::{draft::Error as DraftError, AppError, MailContextError, MailboxError, SidebarError};
use proton_action_queue::action::Action;
use proton_action_queue::queue::ActionError as InternalActionError;
use proton_api_core::login::LoginError;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::response_data::HumanVerificationChallenge;
use proton_core_common::ContactError;
use proton_event_loop::subscriber::SubscriberError;
use proton_event_loop::EventLoopError;

/// Represent all the errors that can be returned by the ProtonMail SDK.
///
/// Currently this is not used in uniffi export as it has its own `ProtonMailError` struct.
/// But for Rust implementations such as TUI this struct is valid.
#[derive(Debug)]
pub struct ProtonMailError {
    pub kind: MailErrorKind,
    pub details: MailErrorDetails,
}

#[derive(Copy, Clone, Debug)]
pub enum MailErrorKind {
    /// User Localizable Error for Invoked Actions
    UserActionError,

    /// User Localizable Error for Session operations
    UserSessionError,

    /// User Localizable Error for Draft new message
    UserDraftError,

    /// User Localizable Error for Login flow
    LoginFlowError,

    /// Localizable Error for Live Event Updates
    UpdateEventError,
}

impl MailErrorKind {
    pub fn with<D: Into<MailErrorDetails>>(self, details: D) -> ProtonMailError {
        ProtonMailError {
            kind: self,
            details: details.into(),
        }
    }
}

/// Categories of errors that can be returned by the ProtonMail SDK.
///
/// It implements From trait for all the internal errors that can occur.
#[derive(Debug)]
pub enum MailErrorDetails {
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

/// Specific Reason for error occurrence
#[derive(Debug)]
pub enum Reason {
    InvalidParameter,
    UnknownLabel,
    UnknownMessage,
    HumanVerificationChallenge(HumanVerificationChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

impl<E: Into<Unexpected>> From<E> for MailErrorDetails {
    fn from(error: E) -> Self {
        Self::Unexpected(error.into())
    }
}

impl From<Reason> for MailErrorDetails {
    fn from(reason: Reason) -> Self {
        Self::Reason(reason)
    }
}

impl From<ApiServiceError> for MailErrorDetails {
    fn from(error: ApiServiceError) -> Self {
        match UserApiServiceError::try_from(error) {
            Ok(api_service_error) => Self::ServerError(api_service_error),
            Err(unexpected) => Self::from(unexpected),
        }
    }
}

impl From<LoginError> for MailErrorDetails {
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

impl From<AppError> for MailErrorDetails {
    fn from(error: AppError) -> Self {
        match error {
            AppError::API(api_service_error) => Self::from(api_service_error),
            AppError::LabelDoesNotHaveRemoteId(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::LabelNotFound(_local_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::InvalidMimeType(_string) => Self::Reason(Reason::InvalidParameter),
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
            AppError::EmptyListOfConversations => Self::Reason(Reason::InvalidParameter),
            AppError::EmptyListOfMessages => Self::Reason(Reason::InvalidParameter),
            AppError::InvalidMobileActions(_) => Self::Reason(Reason::InvalidParameter),
            AppError::MessageHasNoRemoteId(_local_id) => Self::Network,
            AppError::MessageMissing(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownMessage(_remote_id) => Self::Unexpected(Unexpected::Unknown),
            AppError::NoConversationWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::NoMessageWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::UserNotFound => Self::Reason(Reason::InvalidParameter),
            AppError::MessageBodyMissing(_) => Self::Unexpected(Unexpected::Database),
            AppError::RmpDeserialization(_rmp_error) => Self::Unexpected(Unexpected::Internal),
            AppError::RmpSerialization(_rmp_error) => Self::Unexpected(Unexpected::Internal),
        }
    }
}

impl From<MailContextError> for MailErrorDetails {
    fn from(error: MailContextError) -> Self {
        match error {
            MailContextError::Crypto | MailContextError::KeyChainHasNoKey => {
                Self::Unexpected(Unexpected::Crypto)
            }
            MailContextError::Login(login_error) => Self::from(login_error),
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

impl From<DraftError> for MailErrorDetails {
    fn from(error: DraftError) -> Self {
        match error {
            DraftError::UserHasNoAddresses => Self::Unexpected(Unexpected::Database),
            DraftError::AddressNotFound(_remote_id) => Self::Unexpected(Unexpected::Database),
            DraftError::MessageNotADraft(_local_id) => {
                Self::Unexpected(Unexpected::InvalidArgument)
            }
            DraftError::CreateMetadataNotFound(_local_id) => Self::Unexpected(Unexpected::Database),
            DraftError::MessageBodyMissing(_local_id) => Self::Unexpected(Unexpected::Database),
            DraftError::AttachmentDoesNotHaveKeyPackets(_local_id) => {
                Self::Unexpected(Unexpected::InvalidArgument)
            }
            DraftError::ReplyOrForwardToDraft(_local_id) => {
                Self::Unexpected(Unexpected::InvalidArgument)
            }
            DraftError::MetadataNotFound(_metadata_id) => Self::Unexpected(Unexpected::Database),
        }
    }
}

impl From<EventLoopError> for MailErrorDetails {
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

impl From<SubscriberError> for MailErrorDetails {
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

impl From<ContactError> for MailErrorDetails {
    fn from(error: ContactError) -> Self {
        match error {
            ContactError::CardNotFound(_string) => Self::Reason(Reason::InvalidParameter),
            ContactError::ContactCardRemoteIdNotPresent(_string)
            | ContactError::FullContactNotFound(_string) => Self::Unexpected(Unexpected::Database),
            ContactError::Validation(_vcard_validation_error) => {
                Self::Unexpected(Unexpected::Unknown) // TODO: This will be changed in the future work on contacts
            }
        }
    }
}

impl From<ActionError> for MailErrorDetails {
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

impl<T> From<InternalActionError<T>> for MailErrorDetails
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

impl From<MailboxError> for MailErrorDetails {
    fn from(error: MailboxError) -> Self {
        match error {
            // Mailbox::new:     can't load Label from database
            // Mailbox::refresh: can't load Label from database
            // Mailbox::sync:    can't load Label from database
            MailboxError::LabelNotFound(_local_label_id) => Self::Reason(Reason::UnknownLabel),
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
        }
    }
}

impl From<SidebarError> for MailErrorDetails {
    fn from(error: SidebarError) -> Self {
        match error {
            SidebarError::MailContext(mail_context_error) => Self::from(mail_context_error),
            SidebarError::Stash(stash_error) => Self::from(stash_error),
            SidebarError::AppError(app_error) => Self::from(app_error),
        }
    }
}
