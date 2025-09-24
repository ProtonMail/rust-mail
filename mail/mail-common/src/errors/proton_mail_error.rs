use super::mail_error_reason::*;
use crate::actions::MailActionError;
use crate::draft::{
    AttachmentRemoveError, AttachmentUploadError, CancelScheduleSendError, ExpirationError,
    PackageError, PasswordError, SendError, SenderAddressChangeError,
};
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::Unexpected;
use crate::mail_scroller::MailScrollerError;
use crate::{
    AppError, MailContextError, SidebarError, draft::DiscardError as DraftDiscardError,
    draft::Error as DraftError, draft::ExpirationError as DraftExpirationError,
    draft::OpenError as DraftOpenError, draft::PasswordError as DraftPasswordError,
    draft::SaveError as DraftSaveError, draft::SendError as DraftSendError,
    draft::UndoError as DraftUndoError,
};
use proton_action_queue::action::Action;
use proton_action_queue::queue::{ActionError as InternalActionError, MultiActionError};
use proton_core_api::service::ApiServiceError;
use proton_core_common::ContactError;
use proton_core_common::device_registration::RegisteredDeviceTaskError;
use proton_core_common::models::LabelError;
use proton_core_common::pin_code::PinError;
use proton_event_loop::EventLoopError;
use proton_event_loop::subscriber::SubscriberError;

/// Categories of errors that can be returned by the ProtonMail SDK.
///
/// It implements From trait for all the internal errors that can occur.
#[derive(Debug)]
pub enum ProtonMailError {
    /// This error is related with the arguments (i.e. like a Message id who does not exist)
    Reason(MailErrorReason),
    /// This error come from the Backend (i.e. like a 404 error)
    ServerError(UserApiServiceError),
    /// This error come form network (i.e. like can't connect to backend)
    Network,
    /// Something unexpected happened
    Unexpected(Unexpected),
    /// One or more actions can not be processed.
    NonProcessableActions,
}

/// When `proton_mail_error_log` feature is enabled, this guard is used to prevent
/// nested conversions to re-log the same error multiple times.
///
/// The first attempt at converting a type will capture the allowed to log value and then release it
/// once it is done. Subsequent attempts in nested conversions will fail.
///
#[allow(dead_code)]
struct LogStackGuard(bool);

impl LogStackGuard {
    #[cfg(feature = "proton_mail_error_log")]
    fn new() -> Self {
        Self(LOG_STACK.replace(false))
    }

    #[cfg(not(feature = "proton_mail_error_log"))]
    fn new() -> Self {
        Self(true)
    }

    #[cfg(feature = "proton_mail_error_log")]
    fn can_log(&self) -> bool {
        self.0
    }
}

impl Drop for LogStackGuard {
    fn drop(&mut self) {
        #[cfg(feature = "proton_mail_error_log")]
        LOG_STACK.set(self.0);
    }
}

#[cfg(feature = "proton_mail_error_log")]
thread_local! {
    static LOG_STACK: std::cell::Cell<bool> = const{ std::cell::Cell::new(true) };
}

#[cfg(feature = "proton_mail_error_log")]
fn log_error<T: std::error::Error>(value: &T) -> LogStackGuard {
    let guard = LogStackGuard::new();
    if guard.can_log() {
        tracing::error!("ProtonMailError::From: {value:?}");
    }
    guard
}

#[cfg(not(feature = "proton_mail_error_log"))]
fn log_error<T: std::error::Error>(_: &T) -> LogStackGuard {
    LogStackGuard::new()
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
        let _guard = log_error(&error);
        if error.is_network_failure() {
            return Self::Network;
        }

        match UserApiServiceError::try_from(error) {
            Ok(api_service_error) => Self::ServerError(api_service_error),

            Err(unexpected) => Self::from(unexpected),
        }
    }
}

impl From<proton_account_api::ApiError> for ProtonMailError {
    fn from(error: proton_account_api::ApiError) -> Self {
        let _guard = log_error(&error);
        match error {
            proton_account_api::ApiError::Muon(error) => Self::from(ApiServiceError::from(error)),
            proton_account_api::ApiError::Status(error) => Self::from(ApiServiceError::from(error)),
            proton_account_api::ApiError::Serialization(_) => Self::from(Unexpected::Internal),
            proton_account_api::ApiError::Internal(_) => Self::from(Unexpected::Internal),
        }
    }
}

impl From<PinError> for ProtonMailError {
    fn from(value: PinError) -> Self {
        let _guard = log_error(&value);
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
            PinError::CoreContext(core_context_error) => {
                MailContextError::from(core_context_error).into()
            }
            PinError::IoError(_io_error) => Self::Unexpected(Unexpected::FileSystem),
        }
    }
}

impl From<AppError> for ProtonMailError {
    fn from(error: AppError) -> Self {
        log_error(&error);
        match error {
            AppError::API(api_service_error) => Self::from(api_service_error),
            AppError::LabelDoesNotHaveRemoteId(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::LabelNotFound(_local_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::InvalidMimeType(_string) => Self::Unexpected(Unexpected::InvalidArgument),
            AppError::RemoteLabelDoesNotExist(_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::LocalLabelHasNoCounters(_label_id) => Self::Unexpected(Unexpected::Internal),
            AppError::IO(io_error) => Self::from(io_error),
            AppError::Stash(stash_error) => Self::from(stash_error),
            AppError::Label(label_error) => Self::from(label_error),
            AppError::Other(_string) => Self::Unexpected(Unexpected::Unknown),
            AppError::LocalIdNotFound(_string, _remote_id) => {
                Self::Unexpected(Unexpected::Database)
            }
            AppError::AddressHasNoRemoteId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentMissing(_string) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationDoesNotHaveLabel(_, _) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationNotFound(_) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationHasNoMessages(_) => Self::Unexpected(Unexpected::Database),
            AppError::ConversationHasNoRemoteId(_local_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AppError::EmptyListOfConversations => Self::reason(OtherErrorReason::InvalidParameter),
            AppError::EmptyListOfMessages => Self::reason(OtherErrorReason::InvalidParameter),
            AppError::InvalidMobileActions(_) => Self::reason(OtherErrorReason::InvalidParameter),
            AppError::InvalidSnoozeLocation(_) => Self::reason(MailErrorReason::SnoozeReason(
                SnoozeErrorReason::InvalidSnoozeLocation,
            )),
            AppError::SnoozeTimeInThePast => Self::reason(MailErrorReason::SnoozeReason(
                SnoozeErrorReason::SnoozeTimeInThePast,
            )),
            AppError::CouldNotCalculateSnoozeOptions => Self::Unexpected(Unexpected::Internal),
            AppError::AddressMissing(_) => Self::Unexpected(Unexpected::Database),
            AppError::MessageHasNoRemoteId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::MessageMissing(_) => Self::Unexpected(Unexpected::Database),
            AppError::UnknownCid(_, _) => Self::reason(ActionErrorReason::UnknownContentId),
            AppError::AttachmentHasNoAddressId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentMissingKeyPackets(_) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentIsNotInCache(_) => Self::Unexpected(Unexpected::Internal),
            AppError::AttachmentDecryption(_) => Self::Unexpected(Unexpected::Crypto),
            AppError::AttachmentDecryptionIO(_) => Self::Unexpected(Unexpected::Os),
            AppError::AttachmentHasNoRemoteId(_) => Self::Unexpected(Unexpected::Internal),
            AppError::ActionError(_) => Self::Unexpected(Unexpected::Internal),
            AppError::ConversationDoesNotExistOnServer(_) => Self::Unexpected(Unexpected::Api),
        }
    }
}

impl From<MailContextError> for ProtonMailError {
    fn from(error: MailContextError) -> Self {
        let _guard = log_error(&error);
        match error {
            MailContextError::AccountMissing(_) => Self::Unexpected(Unexpected::Database),
            MailContextError::SettingsMissing(_) => Self::Unexpected(Unexpected::Database),
            MailContextError::SessionMissing(_) => Self::Unexpected(Unexpected::Database),
            MailContextError::IntoTransactionError(_) => Self::Unexpected(Unexpected::Database),
            MailContextError::Crypto | MailContextError::KeyChainHasNoKey => {
                Self::Unexpected(Unexpected::Crypto)
            }
            MailContextError::Pin(pin_error) => Self::from(pin_error),
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
            MailContextError::InitMediatorError => Self::Unexpected(Unexpected::Internal),
            MailContextError::UserContextNotInitialized(user_id) => Self::reason(
                ContextErrorReason::UserContextNotInitialized(user_id.into_inner()),
            ),
            MailContextError::LostContext => Self::Unexpected(Unexpected::Internal),
            MailContextError::Rsvp(_) => Self::Unexpected(Unexpected::Unknown),
            MailContextError::MailScroller(mail_scroller_error) => Self::from(mail_scroller_error),
            MailContextError::UrlParseError(_) => Self::Unexpected(Unexpected::Internal),
            MailContextError::NonProcessableActions(_) => Self::NonProcessableActions,
            MailContextError::NetworkMonitorService(_) => Self::Unexpected(Unexpected::Internal),
        }
    }
}

impl From<MailScrollerError> for ProtonMailError {
    fn from(error: MailScrollerError) -> Self {
        let _guard = log_error(&error);
        match error {
            MailScrollerError::NotSynced => Self::reason(MailScrollerErrorReason::NotSynced),
        }
    }
}

impl From<DraftError> for ProtonMailError {
    fn from(value: DraftError) -> Self {
        let _guard = log_error(&value);
        match value {
            DraftError::Open(v) => v.into(),
            DraftError::Save(v) => v.into(),
            DraftError::Send(v) => v.into(),
            DraftError::Discard(v) => v.into(),
            DraftError::Undo(v) => v.into(),
            DraftError::AttachmentUpload(v) => v.into(),
            DraftError::AttachmentRemove(v) => match v {
                AttachmentRemoveError::MetadataNotFound(_)
                | AttachmentRemoveError::AttachmentMetadataNotFound(_) => {
                    Self::Unexpected(Unexpected::Draft)
                }
            },
            DraftError::CancelScheduleSend(v) => v.into(),
            DraftError::SenderAddressChange(v) => v.into(),
            DraftError::Password(v) => v.into(),
            DraftError::Expiration(v) => v.into(),
            DraftError::Actor => ProtonMailError::Unexpected(Unexpected::Draft),
            DraftError::Recipient(_) => {
                // Recipient errors are not directly exposed via uniffi, we will
                // handle this later to avoid to many api breaks.
                ProtonMailError::Unexpected(Unexpected::Draft)
            }
        }
    }
}

impl From<DraftOpenError> for ProtonMailError {
    fn from(value: DraftOpenError) -> Self {
        let _guard = log_error(&value);
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
            DraftOpenError::ShareExtensionStubDraftMissing => {
                Self::Unexpected(Unexpected::Internal)
            }
        }
    }
}

impl From<DraftSendError> for ProtonMailError {
    fn from(value: DraftSendError) -> Self {
        let _guard = log_error(&value);
        match value {
            DraftSendError::MessageIsNotADraft(_) => Self::Reason(
                MailErrorReason::DraftSendReason(DraftSendErrorReason::MessageIsNotADraft),
            ),
            DraftSendError::MessageBodyMissing(_) => Self::Unexpected(Unexpected::Internal),
            DraftSendError::LocalDraftWithoutMessage => Self::Unexpected(Unexpected::Internal),
            DraftSendError::SendMessage(v) => Self::from(v),
            DraftSendError::NoRecipients => Self::Reason(MailErrorReason::DraftSendReason(
                DraftSendErrorReason::NoRecipients,
            )),
            DraftSendError::MetadataNotFound(_) | DraftSendError::DraftDoesNotExistOnServer => {
                Self::Reason(MailErrorReason::DraftSendReason(
                    DraftSendErrorReason::MessageDoesNotExist,
                ))
            }
            DraftSendError::MissingAttachmentUploads => Self::Reason(
                MailErrorReason::DraftSendReason(DraftSendErrorReason::MissingAttachmentUploads),
            ),
            DraftSendError::ScheduleSendExpired => Self::Reason(MailErrorReason::DraftSendReason(
                DraftSendErrorReason::ScheduleSendExpired,
            )),
            DraftSendError::ScheduleSendMessageLimitExceeded => {
                Self::Reason(MailErrorReason::DraftSendReason(
                    DraftSendErrorReason::ScheduleSendMessageLimitExceeded,
                ))
            }
            DraftSendError::EOPasswordDecrypt => Self::Reason(MailErrorReason::DraftSendReason(
                DraftSendErrorReason::EOPasswordDecrypt,
            )),
            DraftSendError::ExpirationTimeTooSoon => Self::Reason(
                MailErrorReason::DraftSendReason(DraftSendErrorReason::ExpirationTimeTooSoon),
            ),
            SendError::MessageTooLarge => Self::Reason(MailErrorReason::DraftSendReason(
                DraftSendErrorReason::MessageTooLarge,
            )),
        }
    }
}

impl From<DraftSaveError> for ProtonMailError {
    fn from(value: DraftSaveError) -> Self {
        let _guard = log_error(&value);
        match value {
            DraftSaveError::AddressNotFound(_) => Self::Unexpected(Unexpected::Internal),
            DraftSaveError::AddressWithoutPrimaryKey(v) => {
                Self::Reason(MailErrorReason::DraftSaveReason(
                    DraftSaveErrorReason::AddressDoesNotHavePrimaryKey(v),
                ))
            }
            DraftSaveError::MessageNotADraft(_) => Self::Reason(MailErrorReason::DraftSaveReason(
                DraftSaveErrorReason::MessageIsNotADraft,
            )),
            DraftSaveError::AttachmentDoesNotHaveKeyPackets(_) => {
                Self::Unexpected(Unexpected::InvalidArgument)
            }
            DraftSaveError::AlreadySent => Self::Reason(MailErrorReason::DraftSaveReason(
                DraftSaveErrorReason::MessageAlreadySent,
            )),
            DraftSaveError::MetadataNotFound(_) | DraftSaveError::DraftDoesNotExistOnServer => {
                Self::Reason(MailErrorReason::DraftSaveReason(
                    DraftSaveErrorReason::MessageDoesNotExist,
                ))
            }
        }
    }
}

impl From<DraftUndoError> for ProtonMailError {
    fn from(value: DraftUndoError) -> Self {
        let _guard = log_error(&value);
        match value {
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
        let _guard = log_error(&value);
        match value {
            DraftDiscardError::DeleteFailed => Self::Reason(MailErrorReason::DraftDiscardReason(
                DraftDiscardErrorReason::MessageDoesNotExist,
            )),
            DraftDiscardError::MetadataNotFound(_)
            | DraftDiscardError::DraftDoesNotExistOnServer => Self::Reason(
                MailErrorReason::DraftDiscardReason(DraftDiscardErrorReason::MessageDoesNotExist),
            ),
        }
    }
}

impl From<AttachmentUploadError> for ProtonMailError {
    fn from(value: AttachmentUploadError) -> Self {
        let _guard = log_error(&value);
        match value {
            AttachmentUploadError::MetadataNotFound(_)
            | AttachmentUploadError::MessageDoesNotExist => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::MessageDoesNotExist,
                ))
            }
            AttachmentUploadError::MessageDoesNotExistOnServer(_) => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::MessageDoesNotExistOnServer,
                ))
            }
            AttachmentUploadError::AttachmentDataMissing(_) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AttachmentUploadError::MissingContentId(_) => Self::Unexpected(Unexpected::Internal),
            AttachmentUploadError::Crypto(_) => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::Crypto,
                ))
            }
            AttachmentUploadError::ExistingUploadActionExist(_) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AttachmentUploadError::AttachmentAlreadyUploaded(_) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AttachmentUploadError::TooManyAttachments => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::TooManyAttachments,
                ))
            }
            AttachmentUploadError::MessageAlreadySent => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::MessageAlreadySent,
                ))
            }
            AttachmentUploadError::AttachmentMetadataNotFound(_)
            | AttachmentUploadError::AttachmentMetadataNotFoundCid(_) => {
                Self::Unexpected(Unexpected::Internal)
            }
            AttachmentUploadError::AttachmentTooLarge => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::AttachmentTooLarge,
                ))
            }
            AttachmentUploadError::RetryInvalidState(_) => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::RetryInvalidState,
                ))
            }
            AttachmentUploadError::TotalAttachmentSizeTooLarge => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::TotalAttachmentSizeTooLarge,
                ))
            }
            AttachmentUploadError::Timeout => {
                Self::Reason(MailErrorReason::DraftAttachmentUploadReason(
                    DraftAttachmentUploadErrorReason::Timeout,
                ))
            }
        }
    }
}

impl From<PackageError> for ProtonMailError {
    fn from(value: PackageError) -> Self {
        let _guard = log_error(&value);
        let draft_reason = match value {
            PackageError::RecipientEmailInvalid(e) => {
                DraftSendErrorReason::RecipientEmailInvalid(e)
            }
            PackageError::ProtonRecipientDoesNotExist(e) => {
                DraftSendErrorReason::ProtonRecipientDoesNotExist(e)
            }
            v => DraftSendErrorReason::PackageError(v.to_string()),
        };

        Self::Reason(MailErrorReason::DraftSendReason(draft_reason))
    }
}

impl From<CancelScheduleSendError> for ProtonMailError {
    fn from(value: CancelScheduleSendError) -> Self {
        let _guard = log_error(&value);
        match value {
            CancelScheduleSendError::TimedOut | CancelScheduleSendError::MetadataNotFound(_) => {
                Self::Unexpected(Unexpected::Internal)
            }
            CancelScheduleSendError::MessageNotFound(_) => {
                Self::Reason(MailErrorReason::DraftCancelScheduleSendReason(
                    DraftCancelScheduleSendErrorReason::MessageDoesNotExist,
                ))
            }
            CancelScheduleSendError::MessageIsNotScheduled(_) => {
                Self::Reason(MailErrorReason::DraftCancelScheduleSendReason(
                    DraftCancelScheduleSendErrorReason::MessageNotScheduled,
                ))
            }
            CancelScheduleSendError::AlreadySent(_) => {
                Self::Reason(MailErrorReason::DraftCancelScheduleSendReason(
                    DraftCancelScheduleSendErrorReason::MessageAlreadySent,
                ))
            }
        }
    }
}

impl From<SenderAddressChangeError> for ProtonMailError {
    fn from(value: SenderAddressChangeError) -> Self {
        let _guard = log_error(&value);
        match value {
            SenderAddressChangeError::AddressNotFound(_) => Self::Unexpected(Unexpected::Internal),
            SenderAddressChangeError::AddressHasNoRemoteId(_)
            | SenderAddressChangeError::AddressNotSendEnabled(_) => {
                Self::Reason(MailErrorReason::DraftSenderAddressChangeReason(
                    DraftSenderAddressChangeErrorReason::AddressNotSendEnabled,
                ))
            }
            SenderAddressChangeError::AddressDisabled(_) => {
                Self::Reason(MailErrorReason::DraftSenderAddressChangeReason(
                    DraftSenderAddressChangeErrorReason::AddressDisabled,
                ))
            }
            SenderAddressChangeError::AddressEmailNotFound(v) => {
                Self::Reason(MailErrorReason::DraftSenderAddressChangeReason(
                    DraftSenderAddressChangeErrorReason::AddressWithEmailNotFound(v),
                ))
            }
        }
    }
}

impl From<DraftPasswordError> for ProtonMailError {
    fn from(value: DraftPasswordError) -> Self {
        let _guard = log_error(&value);
        match value {
            PasswordError::MetadataNotFound(_) => Self::Unexpected(Unexpected::Internal),
            PasswordError::PasswordTooShort => Self::reason(MailErrorReason::DraftPasswordReason(
                DraftPasswordErrorReason::PasswordTooShort,
            )),
            PasswordError::Encryption => Self::Unexpected(Unexpected::Crypto),
            PasswordError::Decryption => Self::Unexpected(Unexpected::Crypto),
        }
    }
}

impl From<EventLoopError> for ProtonMailError {
    fn from(error: EventLoopError) -> Self {
        let _guard = log_error(&error);
        match error {
            EventLoopError::StoreRead(anyhow) | EventLoopError::StoreWrite(anyhow) => {
                Self::from(anyhow)
            }
            EventLoopError::Provider(api_service_error) => Self::from(api_service_error),
            EventLoopError::Subscriber(_string, subscriber_error) => Self::from(subscriber_error),
            EventLoopError::Refresh(_, _) => {
                Self::Reason(MailErrorReason::EventReason(EventErrorReason::Refresh))
            }
            EventLoopError::Register(_) => Self::Unexpected(Unexpected::Internal),
            EventLoopError::Deserialize(anyhow) => Self::from(anyhow),
            EventLoopError::Actor => Self::Unexpected(Unexpected::Internal),
        }
    }
}

impl From<SubscriberError> for ProtonMailError {
    fn from(error: SubscriberError) -> Self {
        let _guard = log_error(&error);
        match error {
            SubscriberError::Api(api_service_error) => Self::from(api_service_error),
            SubscriberError::Other(_) => {
                Self::Reason(MailErrorReason::EventReason(EventErrorReason::Subscriber))
            }
            SubscriberError::StashError(stash_error) => Self::from(stash_error),
        }
    }
}

impl From<ContactError> for ProtonMailError {
    fn from(error: ContactError) -> Self {
        let _guard = log_error(&error);
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
        let _guard = log_error(&error);
        match error {
            MailActionError::Http(api_service_error) => Self::from(api_service_error),
            MailActionError::Stash(stash_error) => Self::from(stash_error),
            MailActionError::App(app_error) => Self::from(app_error),
            MailActionError::NoInput => Self::Unexpected(Unexpected::Internal),
            MailActionError::Label(label_error) => Self::from(label_error),
            MailActionError::Other(anyhow) => Self::from(anyhow),
            MailActionError::LostContext => Self::Unexpected(Unexpected::Queue),
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
        let _guard = log_error(&error);
        match error {
            #[allow(clippy::useless_conversion, reason = "It is not useless, clippy")]
            InternalActionError::Action(error) => Self::from(error.into()),
            InternalActionError::Queue(error) => Self::from(error),
        }
    }
}

impl From<MultiActionError> for ProtonMailError {
    fn from(error: MultiActionError) -> Self {
        let _guard = log_error(&error);
        match error {
            #[allow(clippy::useless_conversion, reason = "It is not useless, clippy")]
            MultiActionError::Action(error) => Self::from(error),
            MultiActionError::Queue(error) => Self::from(error),
        }
    }
}

impl From<SidebarError> for ProtonMailError {
    fn from(error: SidebarError) -> Self {
        let _guard = log_error(&error);
        match error {
            SidebarError::MailContext(mail_context_error) => Self::from(mail_context_error),
            SidebarError::Stash(stash_error) => Self::from(stash_error),
            SidebarError::AppError(app_error) => Self::from(app_error),
        }
    }
}

impl From<LabelError> for ProtonMailError {
    fn from(error: LabelError) -> Self {
        let _guard = log_error(&error);
        match error {
            LabelError::API(api_service_error) => Self::from(api_service_error),
            LabelError::Stash(stash_error) => Self::from(stash_error),
            LabelError::CouldNotResolveRemoteLabel(_local_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            LabelError::CouldNotResolveLocalLabel(_label_id) => {
                Self::Unexpected(Unexpected::Internal)
            }
            LabelError::LabelWithoutIds => Self::Unexpected(Unexpected::Internal),
        }
    }
}

impl From<RegisteredDeviceTaskError> for ProtonMailError {
    fn from(error: RegisteredDeviceTaskError) -> Self {
        let _guard = log_error(&error);
        match error {
            RegisteredDeviceTaskError::CreateContext(core_context_error) => {
                MailContextError::from(core_context_error).into()
            }
            RegisteredDeviceTaskError::Stash(stash_error) => Self::from(stash_error),
            RegisteredDeviceTaskError::DeviceStream(_) => Self::Unexpected(Unexpected::Internal),
            RegisteredDeviceTaskError::SessionStreamEnded => Self::Unexpected(Unexpected::Internal),
            RegisteredDeviceTaskError::Crypto => Self::Unexpected(Unexpected::Crypto),
            RegisteredDeviceTaskError::API(_) => Self::Unexpected(Unexpected::Api),
        }
    }
}

impl From<DraftExpirationError> for ProtonMailError {
    fn from(error: DraftExpirationError) -> Self {
        let _guard = log_error(&error);
        match error {
            ExpirationError::MetadataNotFound(_) => ProtonMailError::Unexpected(Unexpected::Draft),
            ExpirationError::ExpirationTimeInThePast => {
                ProtonMailError::Reason(MailErrorReason::DraftExpirationReason(
                    DraftExpirationErrorReason::ExpirationTimeInThePast,
                ))
            }
            ExpirationError::ExpirationTimeExceeds28Days => {
                ProtonMailError::Reason(MailErrorReason::DraftExpirationReason(
                    DraftExpirationErrorReason::ExpirationTimeExceeds28Days,
                ))
            }
            ExpirationError::ExpirationTimeLessThan15Min => {
                ProtonMailError::Reason(MailErrorReason::DraftExpirationReason(
                    DraftExpirationErrorReason::ExpirationTimeLessThan15Min,
                ))
            }
        }
    }
}
