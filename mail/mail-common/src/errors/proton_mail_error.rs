use super::mail_error_reason::*;
use crate::actions::MailActionError;
use crate::draft::{AttachmentError, PackageError, SaveOrSendError};
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::Unexpected;
use crate::{
    AppError, MailContextError, MailboxError, SidebarError,
    draft::DiscardError as DraftDiscardError, draft::Error as DraftError,
    draft::OpenError as DraftOpenError, draft::SaveOrSendError as DraftSaveOrSendError,
    draft::UndoError as DraftUndoError,
};
use proton_action_queue::action::Action;
use proton_action_queue::queue::ActionError as InternalActionError;
use proton_api_core::login::LoginError;
use proton_api_core::service::ApiServiceError;
use proton_core_common::ContactError;
use proton_core_common::models::LabelError;
use proton_core_common::pin_code::PinError;
use proton_event_loop::EventLoopError;
use proton_event_loop::subscriber::SubscriberError;
use tracing::error;

/// Categories of errors that can be returned by the ProtonMail SDK.
///
/// It implements From trait for all the internal errors that can occur.
#[derive(Debug)]
pub enum ProtonMailError {
    /// This error is related with the arguments (i.e. like a Message id who does not exist)
    Reason(MailErrorReason),
    /// This error is related with the session (i.e. like a session expired)
    SessionExpired,
    /// This error come from the Backend (i.e. like a 404 error)
    ServerError(UserApiServiceError),
    /// This error come form network (i.e. like can't connect to backend)
    Network,
    /// Something unexpected happened
    Unexpected(Unexpected),
}

impl ProtonMailError {
    /// Shorthand for creating a `ProtonMailError::Reason`.
    pub fn reason<R: Into<MailErrorReason>>(reason: R) -> Self {
        Self::Reason(reason.into())
    }
}

impl<E: Into<Unexpected>> From<E> for ProtonMailError {
    fn from(error: E) -> Self {
        Self::Unexpected(error.into())
    }
}

impl From<MailErrorReason> for ProtonMailError {
    fn from(reason: MailErrorReason) -> Self {
        Self::Reason(reason)
    }
}

impl From<ApiServiceError> for ProtonMailError {
    fn from(error: ApiServiceError) -> Self {
        if error.is_network_failure() {
            return Self::Network;
        }

        match UserApiServiceError::try_from(error) {
            Ok(api_service_error) => Self::ServerError(api_service_error),

            Err(unexpected) => {
                error!("unexpected error from ApiServiceError: {unexpected:?}");
                Self::from(unexpected)
            }
        }
    }
}

impl From<PinError> for ProtonMailError {
    fn from(value: PinError) -> Self {
        match value {
            PinError::TooShort => Self::reason(PinSetErrorReason::TooShort),
            PinError::TooLong => Self::reason(PinSetErrorReason::TooLong),
            PinError::Malformed => Self::reason(PinSetErrorReason::Malformed),
            PinError::MissingPinMetadata => Self::Unexpected(Unexpected::Internal),
            PinError::MissingPinHash => Self::Unexpected(Unexpected::Internal),
            PinError::TooManyAttempts => Self::reason(PinAuthErrorReason::TooManyAttempts),
            PinError::TooFrequentAttempts => Self::reason(PinAuthErrorReason::TooFrequentAttempts),
            PinError::IncorrectPin => Self::reason(PinAuthErrorReason::IncorrectPin),
            PinError::HashError(_hashing_error) => Self::Unexpected(Unexpected::Crypto),
            PinError::Keychain(_core_context_error) => Self::Unexpected(Unexpected::Crypto),
            PinError::StashError(_stash_error) => Self::Unexpected(Unexpected::Database),
            PinError::JoinError(_join_error) => Self::Unexpected(Unexpected::Internal),
        }
    }
}

impl From<LoginError> for ProtonMailError {
    fn from(error: LoginError) -> Self {
        match error {
            LoginError::InvalidState => Self::Unexpected(Unexpected::Internal),
            LoginError::FlowLogin(api_service_error)
            | LoginError::FlowTotp(api_service_error)
            | LoginError::FlowFido(api_service_error)
            | LoginError::UserFetch(api_service_error) => Self::from(api_service_error),
            LoginError::MissingPrimaryKey
            | LoginError::KeySecretAuthUpdate(_)
            | LoginError::KeySecretDecryption
            | LoginError::KeySecretDerivation(_) => {
                Self::reason(LoginErrorReason::CantUnlockUserKey)
            }
            LoginError::KeySecretSaltFetch(api_service_error) => match api_service_error {
                // HTTP code 422
                ApiServiceError::UnprocessableEntity(_string1, _string2) => {
                    // TODO(ET-1076): use api_code: 8002 -> InvalidCredentials ; 2005 -> EmptyInput ; other -> Self::from(api_service_error)
                    Self::reason(LoginErrorReason::InvalidCredentials)
                }
                _ => Self::from(api_service_error),
            },
            LoginError::ServerProof(_string) | LoginError::SrpProof(_string) => {
                Self::reason(LoginErrorReason::InvalidCredentials)
            }
            LoginError::UnsupportedTfa => Self::Reason(LoginErrorReason::UnsupportedTfa.into()),
            LoginError::WrongMailboxPassword => Self::Unexpected(Unexpected::Internal),
            LoginError::AuthStore(store_error) => Self::from(store_error),
        }
    }
}

impl From<AppError> for ProtonMailError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::API(api_service_error) => Self::from(api_service_error),
            AppError::LabelDoesNotHaveRemoteId(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::LabelNotFound(_local_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::InvalidMimeType(_string) => Self::Unexpected(Unexpected::InvalidArgument),
            AppError::MessageBodyMetadataMissing(_local_massage_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::RemoteLabelDoesNotExist(_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::RemoteLabelHasNoCounters(_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::LocalLabelHasNoCounters(_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::IO(io_error) => Self::from(io_error),
            AppError::Stash(stash_error) => Self::from(stash_error),
            AppError::Label(label_error) => Self::from(label_error),
            AppError::Other(_string) => Self::Unexpected(Unexpected::Unknown),
            AppError::LocalIdNotFound(_string, _remote_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::AddressHasNoRemoteId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::ActionStillQueued(_string) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentMissing(_string) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownAttachment(_) => Self::Unexpected(Unexpected::Unknown),
            AppError::AttachmentDoesNotHaveRemoteId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::ConversationDoesNotHaveLabel(_, _) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationNotFound(_) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationHasNoMessages(_) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationHasNoRemoteId(_local_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::EmptyListOfConversations => Self::reason(OtherErrorReason::InvalidParameter),
            AppError::EmptyListOfMessages => Self::reason(OtherErrorReason::InvalidParameter),
            AppError::InvalidMobileActions(_) => Self::reason(OtherErrorReason::InvalidParameter),
            AppError::MessageHasNoRemoteId(_local_id) => Self::Unexpected(Unexpected::Internal),
            AppError::MessageMissing(_local_id) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownMessage(_remote_id) => Self::Unexpected(Unexpected::Unknown),
            AppError::NoConversationWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::NoMessageWithValidRemoteIdFoundInPage => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::UserNotFound => Self::reason(OtherErrorReason::InvalidParameter),
            AppError::MessageBodyMissing(_) => Self::Unexpected(Unexpected::Database),
            AppError::RmpDeserialization(_rmp_error) => Self::Unexpected(Unexpected::Internal),
            AppError::RmpSerialization(_rmp_error) => Self::Unexpected(Unexpected::Internal),
            AppError::UnknownCid(_, _) => Self::reason(ActionErrorReason::UnknownContentId),
            AppError::AttachmentHasNoAddressId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentMissingKeyPackets(_) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentIsNotInCache(_) => Self::Unexpected(Unexpected::Internal),
        }
    }
}

impl From<MailContextError> for ProtonMailError {
    fn from(error: MailContextError) -> Self {
        match error {
            MailContextError::AccountMissing(_) => Self::Unexpected(Unexpected::Database),
            MailContextError::SettingsMissing(_) => Self::Unexpected(Unexpected::Database),
            MailContextError::SessionMissing(_) => Self::Unexpected(Unexpected::Database),
            MailContextError::IntoTransactionError(_) => Self::Unexpected(Unexpected::Database),
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
            MailContextError::Other(anyhow) => Self::from(anyhow),
            MailContextError::ContactError(contact_error) => Self::from(contact_error),
            MailContextError::Draft(draft_error) => Self::from(draft_error),
            MailContextError::Build(_parse_app_version_error) => {
                Self::Unexpected(Unexpected::Config)
            }
            MailContextError::PGPKeySelection(_encryption_preferences_error) => {
                Self::Unexpected(Unexpected::Crypto)
            }
            MailContextError::DuplicateContext(_remote_id) => {
                Self::reason(ContextErrorReason::DuplicateContext)
            }
            MailContextError::InitializationFailed(_e) =>
            // TODO (ET-2558) Use proper error message. Mobile app must handle it differently.
            {
                Self::Unexpected(Unexpected::Internal)
            }
            MailContextError::Label(label_error) => Self::from(label_error),
            MailContextError::TaskCancelled => Self::Unexpected(Unexpected::Internal),
            MailContextError::MissingContext => Self::Unexpected(Unexpected::Internal),
            MailContextError::QueueWriterGuardExpired => Self::Unexpected(Unexpected::Queue),
            MailContextError::AttachmentEncryption(_) => Self::Unexpected(Unexpected::Crypto),
            MailContextError::CalledFetchedAttachmentOnPgp
            | MailContextError::CalledFetchedAttachmentLocalAttachment
            | MailContextError::InvalidUtf8AttachmentPath(_) => {
                Self::Unexpected(Unexpected::Internal)
            }
        }
    }
}

impl From<DraftError> for ProtonMailError {
    fn from(value: DraftError) -> Self {
        match value {
            DraftError::Open(v) => v.into(),
            DraftError::SaveOrSend(v) => v.into(),
            DraftError::Discard(v) => v.into(),
            DraftError::Undo(v) => v.into(),
            DraftError::Attachment(v) => v.into(),
        }
    }
}

impl From<DraftOpenError> for ProtonMailError {
    fn from(value: DraftOpenError) -> Self {
        match value {
            DraftOpenError::UserHasNoAddresses => Self::Unexpected(Unexpected::Internal),
            DraftOpenError::AddressNotFound(_) => Self::Reason(MailErrorReason::DraftOpenReason(
                DraftOpenErrorReason::AddressNotFound,
            )),
            DraftOpenError::MessageNotADraft(_) => Self::Reason(MailErrorReason::DraftOpenReason(
                DraftOpenErrorReason::MessageIsNotADraft,
            )),
            DraftOpenError::MessageBodyMissing(_) => Self::Reason(
                MailErrorReason::DraftOpenReason(DraftOpenErrorReason::MessageBodyMissing),
            ),
            DraftOpenError::ReplyOrForwardToDraft(_) => Self::Reason(
                MailErrorReason::DraftOpenReason(DraftOpenErrorReason::MessageBodyMissing),
            ),
        }
    }
}

impl From<DraftSaveOrSendError> for ProtonMailError {
    fn from(value: DraftSaveOrSendError) -> Self {
        match value {
            DraftSaveOrSendError::UserHasNoAddresses => Self::Unexpected(Unexpected::Internal),
            DraftSaveOrSendError::AddressNotFound(_) => Self::Unexpected(Unexpected::Internal),
            DraftSaveOrSendError::AddressWithoutPrimaryKey(v) => {
                Self::Reason(MailErrorReason::DraftSaveSendReason(
                    DraftSaveSendErrorReason::AddressDoesNotHavePrimaryKey(v),
                ))
            }
            DraftSaveOrSendError::MessageNotADraft(_) => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::MessageIsNotADraft),
            ),
            DraftSaveOrSendError::MessageBodyMissing(_) => Self::Unexpected(Unexpected::Internal),
            DraftSaveOrSendError::AttachmentDoesNotHaveKeyPackets(_) => {
                Self::Unexpected(Unexpected::InvalidArgument)
            }
            DraftSaveOrSendError::MetadataNotFound(_) => Self::Unexpected(Unexpected::Internal),
            DraftSaveOrSendError::LocalDraftWithoutMessage => {
                Self::Unexpected(Unexpected::Internal)
            }
            DraftSaveOrSendError::AlreadySent => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::AlreadySent),
            ),
            DraftSaveOrSendError::SendMessage(v) => Self::from(v),
            DraftSaveOrSendError::NoRecipients => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::NoRecipients),
            ),
            DraftSaveOrSendError::DraftDoesNotExistOnServer => Self::Reason(
                MailErrorReason::DraftSaveSendReason(DraftSaveSendErrorReason::MessageDoesNotExist),
            ),
            SaveOrSendError::MissingAttachmentUploads => {
                Self::Reason(MailErrorReason::DraftSaveSendReason(
                    DraftSaveSendErrorReason::MissingAttachmentUploads,
                ))
            }
        }
    }
}

impl From<DraftUndoError> for ProtonMailError {
    fn from(value: DraftUndoError) -> Self {
        match value {
            DraftUndoError::MessageNotADraft(_) => Self::Reason(
                MailErrorReason::DraftUndoSendReason(DraftUndoSendErrorReason::MessageIsNotADraft),
            ),
            DraftUndoError::MetadataNotFound(_) => Self::Unexpected(Unexpected::Internal),
            DraftUndoError::MessageCanNotBeUndoSent(_) => {
                Self::Reason(MailErrorReason::DraftUndoSendReason(
                    DraftUndoSendErrorReason::MessageCanNotBeUndoSent,
                ))
            }
            DraftUndoError::SendCanNoLongerBeUndone => {
                Self::Reason(MailErrorReason::DraftUndoSendReason(
                    DraftUndoSendErrorReason::SendCanNoLongerBeUndone,
                ))
            }
            DraftUndoError::DraftDoesNotExistOnServer => Self::Reason(
                MailErrorReason::DraftUndoSendReason(DraftUndoSendErrorReason::MessageDoesNotExist),
            ),
        }
    }
}

impl From<DraftDiscardError> for ProtonMailError {
    fn from(value: DraftDiscardError) -> Self {
        match value {
            DraftDiscardError::MetadataNotFound(_) => Self::Unexpected(Unexpected::Internal),
            DraftDiscardError::DeleteFailed => Self::Reason(MailErrorReason::DraftDiscardReason(
                DraftDiscardErrorReason::MessageDoesNotExist,
            )),
            DraftDiscardError::DraftDoesNotExistOnServer => Self::Reason(
                MailErrorReason::DraftDiscardReason(DraftDiscardErrorReason::MessageDoesNotExist),
            ),
        }
    }
}

impl From<AttachmentError> for ProtonMailError {
    fn from(value: AttachmentError) -> Self {
        match value {
            AttachmentError::MetadataNotFound(_) => Self::Unexpected(Unexpected::Internal),
            AttachmentError::MessageDoesNotExist => {
                Self::Reason(MailErrorReason::DraftAttachmentReason(
                    DraftAttachmentErrorReason::MessageDoesNotExist,
                ))
            }
            AttachmentError::MessageDoesNotExistOnServer(_) => {
                Self::Reason(MailErrorReason::DraftAttachmentReason(
                    DraftAttachmentErrorReason::MessageDoesNotExistOnServer,
                ))
            }
            AttachmentError::AttachmentDataMissing(_) => Self::Unexpected(Unexpected::Internal),
            AttachmentError::MissingContentId(_) => Self::Unexpected(Unexpected::Internal),
            AttachmentError::Crypto(_) => Self::Reason(MailErrorReason::DraftAttachmentReason(
                DraftAttachmentErrorReason::Crypto,
            )),
            AttachmentError::ExistingUploadActionExist(_) => Self::Unexpected(Unexpected::Internal),
            AttachmentError::AttachmentAlreadyUploaded(_) => Self::Unexpected(Unexpected::Internal),
            AttachmentError::TooManyAttachments => {
                Self::Reason(MailErrorReason::DraftAttachmentReason(
                    DraftAttachmentErrorReason::TooManyAttachments,
                ))
            }
            AttachmentError::MessageAlreadySent => {
                Self::Reason(MailErrorReason::DraftAttachmentReason(
                    DraftAttachmentErrorReason::MessageAlreadySent,
                ))
            }
            AttachmentError::AttachmentMetadataNotFound(_) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AttachmentError::AttachmentTooLarge => {
                Self::Reason(MailErrorReason::DraftAttachmentReason(
                    DraftAttachmentErrorReason::AttachmentTooLarge,
                ))
            }
            AttachmentError::RetryInvalidState(_) => {
                Self::Reason(MailErrorReason::DraftAttachmentReason(
                    DraftAttachmentErrorReason::RetryInvalidState,
                ))
            }
        }
    }
}

impl From<PackageError> for ProtonMailError {
    fn from(value: PackageError) -> Self {
        let draft_reason = match value {
            PackageError::RecipientEmailInvalid(e) => {
                DraftSaveSendErrorReason::RecipientEmailInvalid(e)
            }
            PackageError::ProtonRecipientDoesNotExist(e) => {
                DraftSaveSendErrorReason::ProtonRecipientDoesNotExist(e)
            }
            PackageError::UnknownRecipientValidationError(e) => {
                DraftSaveSendErrorReason::UnknownRecipientValidationError(e)
            }
            v => DraftSaveSendErrorReason::PackageError(v.to_string()),
        };

        Self::Reason(MailErrorReason::DraftSaveSendReason(draft_reason))
    }
}

impl From<EventLoopError> for ProtonMailError {
    fn from(error: EventLoopError) -> Self {
        match error {
            EventLoopError::StoreRead(anyhow) | EventLoopError::StoreWrite(anyhow) => {
                Self::from(anyhow)
            }
            EventLoopError::Provider(api_service_error) => Self::from(api_service_error),
            EventLoopError::Subscriber(_string, subscriber_error) => Self::from(subscriber_error),
            EventLoopError::Refresh => {
                Self::Reason(MailErrorReason::EventReason(EventErrorReason::Refresh))
            }
        }
    }
}

impl From<SubscriberError> for ProtonMailError {
    fn from(error: SubscriberError) -> Self {
        match error {
            SubscriberError::Api(api_service_error) => Self::from(api_service_error),
            SubscriberError::Other(_) => {
                Self::Reason(MailErrorReason::EventReason(EventErrorReason::Subscriber))
            }
            SubscriberError::Send | SubscriberError::Receive => {
                Self::Unexpected(Unexpected::Internal)
            }
            SubscriberError::StashError(stash_error) => Self::from(stash_error),
        }
    }
}

impl From<ContactError> for ProtonMailError {
    fn from(error: ContactError) -> Self {
        match error {
            ContactError::CardNotFound(_string) => Self::reason(OtherErrorReason::InvalidParameter),
            ContactError::ContactCardRemoteIdNotPresent(_string)
            | ContactError::FullContactNotFound(_string) => Self::Unexpected(Unexpected::Database),
            ContactError::Validation(_vcard_validation_error) => {
                Self::Unexpected(Unexpected::Unknown) // TODO: This will be changed in the future work on contacts
            }
            ContactError::ContactDoesNotHaveRemoteId(_local_id) => {
                Self::Unexpected(Unexpected::Database)
            }
        }
    }
}

impl From<MailActionError> for ProtonMailError {
    fn from(error: MailActionError) -> Self {
        match error {
            MailActionError::Http(api_service_error) => Self::from(api_service_error),
            MailActionError::Stash(stash_error) => Self::from(stash_error),
            MailActionError::App(app_error) => Self::from(app_error),
            MailActionError::NoInput => Self::Unexpected(Unexpected::Internal),
            MailActionError::Label(label_error) => Self::from(label_error),
            MailActionError::Other(anyhow) => Self::from(anyhow),
            MailActionError::QueueWriterGuardExpired => Self::Unexpected(Unexpected::Queue),
        }
    }
}

impl<T> From<InternalActionError<T>> for ProtonMailError
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

impl From<MailboxError> for ProtonMailError {
    fn from(error: MailboxError) -> Self {
        match error {
            // Mailbox::new:     can't load Label from database
            // Mailbox::refresh: can't load Label from database
            // Mailbox::sync:    can't load Label from database
            MailboxError::LabelNotFound(_local_label_id) => {
                Self::reason(ActionErrorReason::UnknownLabel)
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
            MailboxError::IO(io_error) => Self::from(io_error),
        }
    }
}

impl From<SidebarError> for ProtonMailError {
    fn from(error: SidebarError) -> Self {
        match error {
            SidebarError::MailContext(mail_context_error) => Self::from(mail_context_error),
            SidebarError::Stash(stash_error) => Self::from(stash_error),
            SidebarError::AppError(app_error) => Self::from(app_error),
        }
    }
}

impl From<LabelError> for ProtonMailError {
    fn from(error: LabelError) -> Self {
        match error {
            LabelError::API(api_service_error) => Self::from(api_service_error),
            LabelError::Stash(stash_error) => Self::from(stash_error),
            LabelError::CouldNotResolveRemoteLabel(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            LabelError::CouldNotResolveLocalLabel(_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
        }
    }
}
