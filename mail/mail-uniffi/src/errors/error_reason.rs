use crate::UniffiEnum;
use proton_mail_common::errors::{
    ActionErrorReason as RealActionErrorReason, ContextErrorReason as RealContextErrorReason,
    DraftAttachmentUploadErrorReason as RealDraftAttachmentErrorReason,
    DraftDiscardErrorReason as RealDraftDiscardErrorReason,
    DraftOpenErrorReason as RealDraftOpenErrorReason,
    DraftSaveErrorReason as RealDraftSaveErrorReason,
    DraftSendErrorReason as RealDraftSendErrorReason,
    DraftUndoSendErrorReason as RealDraftUndoSendErrorReason,
    EventErrorReason as RealEventErrorReason, LoginErrorReason as RealLoginErrorReason,
    OtherErrorReason as RealOtherErrorReason, PinAuthErrorReason as RealPinAuthErrorReason,
    PinSetErrorReason as RealPinSetErrorReason,
};

/// Specific Reason for error occurrence within ActionQueue
///
/// This enum is used to represent the specific reason for an error that occurred
/// in order to provide only the necessary information to the user.
#[derive(Debug, UniffiEnum)]
pub enum ActionErrorReason {
    UnknownLabel,
    UnknownMessage,
    UnknownContentId,
}

impl From<RealActionErrorReason> for ActionErrorReason {
    fn from(reason: RealActionErrorReason) -> Self {
        match reason {
            RealActionErrorReason::UnknownLabel => ActionErrorReason::UnknownLabel,
            RealActionErrorReason::UnknownMessage => ActionErrorReason::UnknownMessage,
            RealActionErrorReason::UnknownContentId => ActionErrorReason::UnknownContentId,
        }
    }
}

/// Specific Reason for error occurrence within Session.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling session related operations in order to provide only the necessary
/// information to the user. This error type in common library is named `ContextErrorReason`
/// as context is nomenclature used in the common library.
#[derive(Debug, UniffiEnum)]
pub enum SessionErrorReason {
    UnknownLabel,
    DuplicateContext,
    UserContextNotInitialized,
}

impl From<RealContextErrorReason> for SessionErrorReason {
    fn from(reason: RealContextErrorReason) -> Self {
        match reason {
            RealContextErrorReason::UnknownLabel => SessionErrorReason::UnknownLabel,
            RealContextErrorReason::DuplicateContext => SessionErrorReason::DuplicateContext,
            RealContextErrorReason::UserContextNotInitialized(_) => {
                SessionErrorReason::UserContextNotInitialized
            }
        }
    }
}

/// Specific Reason for error occurrence within Login Flow.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling login related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug, UniffiEnum)]
pub enum LoginErrorReason {
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

impl From<RealLoginErrorReason> for LoginErrorReason {
    fn from(reason: RealLoginErrorReason) -> Self {
        match reason {
            RealLoginErrorReason::InvalidCredentials => LoginErrorReason::InvalidCredentials,
            RealLoginErrorReason::UnsupportedTfa => LoginErrorReason::UnsupportedTfa,
            RealLoginErrorReason::CantUnlockUserKey => LoginErrorReason::CantUnlockUserKey,
        }
    }
}

/// Specific Reason when opening a draft fails.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while drafting a new message in order to provide only the necessary
/// information to the user.
#[derive(Debug, UniffiEnum)]
pub enum DraftOpenErrorReason {
    /// This message no longer exists.
    MessageDoesNotExist,
    /// This message is not a draft
    MessageIsNotADraft,
    /// Attempting to reply or forward to a draft
    ReplyOrForwardDraft,
    /// Could not find the user's address
    AddressNotFound,
    /// Message body is missing
    MessageBodyMissing,
}
impl From<RealDraftOpenErrorReason> for DraftOpenErrorReason {
    fn from(value: RealDraftOpenErrorReason) -> Self {
        match value {
            RealDraftOpenErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
            RealDraftOpenErrorReason::MessageIsNotADraft => Self::MessageIsNotADraft,
            RealDraftOpenErrorReason::ReplyOrForwardDraft => Self::ReplyOrForwardDraft,
            RealDraftOpenErrorReason::AddressNotFound => Self::AddressNotFound,
            RealDraftOpenErrorReason::MessageBodyMissing => Self::MessageBodyMissing,
        }
    }
}

/// Specific Reason when saving a draft
#[derive(Debug, UniffiEnum)]
pub enum DraftSaveErrorReason {
    /// Address does not have a primary key
    AddressDoesNotHavePrimaryKey(String),
    /// Recipient email is invalid
    RecipientEmailInvalid(String),
    /// This Proton recipient does not exist.
    ProtonRecipientDoesNotExist(String),
    /// Some other validation error occurred for this recipient
    UnknownRecipientValidationError(String),
    /// This address is disabled and can't be used for sending
    AddressDisabled(String),
    /// Message was already sent.
    MessageAlreadySent,
    /// This draft was already sent and can't be modified
    AlreadySent,
    /// This message no longer exists.
    MessageDoesNotExist,
    /// Message is not a draft
    MessageIsNotADraft,
}

/// Specific Reason when saving a draft
#[derive(Debug, UniffiEnum)]
pub enum DraftSendErrorReason {
    /// Message has no recipients
    NoRecipients,
    /// Address does not have a primary key
    AddressDoesNotHavePrimaryKey(String),
    /// Recipient email is invalid
    RecipientEmailInvalid(String),
    /// This Proton recipient does not exist.
    ProtonRecipientDoesNotExist(String),
    /// Some other validation error occurred for this recipient
    UnknownRecipientValidationError(String),
    /// This address is disabled and can't be used for sending
    AddressDisabled(String),
    /// Message was already sent.
    MessageAlreadySent,
    /// A packaging error occurred
    PackageError(String),
    /// This draft was already sent and can't be modified
    AlreadySent,
    /// This message no longer exists.
    MessageDoesNotExist,
    /// Message is not a draft
    MessageIsNotADraft,
    /// Message is missing attachment uploads
    MissingAttachmentUploads,
}

impl From<RealDraftSaveErrorReason> for DraftSaveErrorReason {
    fn from(value: RealDraftSaveErrorReason) -> Self {
        match value {
            RealDraftSaveErrorReason::AddressDoesNotHavePrimaryKey(value) => {
                Self::AddressDoesNotHavePrimaryKey(value.into_inner())
            }
            RealDraftSaveErrorReason::RecipientEmailInvalid(value) => {
                Self::RecipientEmailInvalid(value)
            }
            RealDraftSaveErrorReason::ProtonRecipientDoesNotExist(value) => {
                Self::ProtonRecipientDoesNotExist(value)
            }
            RealDraftSaveErrorReason::UnknownRecipientValidationError(value) => {
                Self::UnknownRecipientValidationError(value)
            }
            RealDraftSaveErrorReason::AddressDisabled(value) => Self::AddressDisabled(value),
            RealDraftSaveErrorReason::MessageAlreadySent => Self::MessageAlreadySent,
            RealDraftSaveErrorReason::AlreadySent => Self::AlreadySent,
            RealDraftSaveErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
            RealDraftSaveErrorReason::MessageIsNotADraft => Self::MessageIsNotADraft,
        }
    }
}

impl From<RealDraftSendErrorReason> for DraftSendErrorReason {
    fn from(value: RealDraftSendErrorReason) -> Self {
        match value {
            RealDraftSendErrorReason::NoRecipients => Self::NoRecipients,
            RealDraftSendErrorReason::RecipientEmailInvalid(value) => {
                Self::RecipientEmailInvalid(value)
            }
            RealDraftSendErrorReason::ProtonRecipientDoesNotExist(value) => {
                Self::ProtonRecipientDoesNotExist(value)
            }
            RealDraftSendErrorReason::UnknownRecipientValidationError(value) => {
                Self::UnknownRecipientValidationError(value)
            }
            RealDraftSendErrorReason::PackageError(value) => Self::PackageError(value),
            RealDraftSendErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
            RealDraftSendErrorReason::MessageIsNotADraft => Self::MessageIsNotADraft,
            RealDraftSendErrorReason::MissingAttachmentUploads => Self::MissingAttachmentUploads,
        }
    }
}

/// Specific Reason when attempting to cancel sending of an already sent draft.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while saving or sending a draft in order to provide only the necessary
/// information to the user.
#[derive(Debug, UniffiEnum)]
pub enum DraftUndoSendErrorReason {
    /// Can not undo sent this message
    MessageCanNotBeUndoSent,
    /// The cancellation of sending for this message is no longer possible.
    SendCanNoLongerBeUndone,
    /// Message is not a draft
    MessageIsNotADraft,
    /// This message no longer exists.
    MessageDoesNotExist,
}

impl From<RealDraftUndoSendErrorReason> for DraftUndoSendErrorReason {
    fn from(value: RealDraftUndoSendErrorReason) -> Self {
        match value {
            RealDraftUndoSendErrorReason::MessageCanNotBeUndoSent => Self::MessageCanNotBeUndoSent,
            RealDraftUndoSendErrorReason::SendCanNoLongerBeUndone => Self::SendCanNoLongerBeUndone,
            RealDraftUndoSendErrorReason::MessageIsNotADraft => Self::MessageIsNotADraft,
            RealDraftUndoSendErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
        }
    }
}

/// Specific Reason when attempting to discard a draft.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while saving or sending a draft in order to provide only the necessary
/// information to the user.
#[derive(Debug, UniffiEnum)]
pub enum DraftDiscardErrorReason {
    /// This message does not exist
    MessageDoesNotExist,
    /// Deleting the draft failed
    DeleteFailed,
}

impl From<RealDraftDiscardErrorReason> for DraftDiscardErrorReason {
    fn from(value: RealDraftDiscardErrorReason) -> Self {
        match value {
            RealDraftDiscardErrorReason::DeleteFailed => Self::DeleteFailed,
            RealDraftDiscardErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
        }
    }
}

#[derive(Debug, UniffiEnum)]
pub enum DraftAttachmentUploadErrorReason {
    /// This message no longer exists.
    MessageDoesNotExist,
    /// Message does not exist on the server
    MessageDoesNotExistOnServer,
    /// Failed to encrypt the attachment
    Crypto,
    /// Too Many Attachments
    TooManyAttachments,
    /// Message was already sent.
    MessageAlreadySent,
    /// Attachment is too large
    AttachmentTooLarge,
    /// Upload Retry in invalid state
    RetryInvalidState,
}

impl From<RealDraftAttachmentErrorReason> for DraftAttachmentUploadErrorReason {
    fn from(value: RealDraftAttachmentErrorReason) -> Self {
        match value {
            RealDraftAttachmentErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
            RealDraftAttachmentErrorReason::MessageDoesNotExistOnServer => {
                Self::MessageDoesNotExistOnServer
            }
            RealDraftAttachmentErrorReason::Crypto => Self::Crypto,
            RealDraftAttachmentErrorReason::TooManyAttachments => Self::TooManyAttachments,
            RealDraftAttachmentErrorReason::MessageAlreadySent => Self::MessageAlreadySent,
            RealDraftAttachmentErrorReason::AttachmentTooLarge => Self::AttachmentTooLarge,
            RealDraftAttachmentErrorReason::RetryInvalidState => Self::RetryInvalidState,
        }
    }
}

/// Specific Reason for error occurrence within Event Loop.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling event loop related operations in order to provide only the necessary
/// information to the user.
#[derive(Debug, UniffiEnum)]
pub enum EventErrorReason {
    Refresh,
    Subscriber,
}

impl From<RealEventErrorReason> for EventErrorReason {
    fn from(reason: RealEventErrorReason) -> Self {
        match reason {
            RealEventErrorReason::Subscriber => EventErrorReason::Subscriber,
            RealEventErrorReason::Refresh => EventErrorReason::Refresh,
        }
    }
}

/// Specific Reason for error occurrence while creating user's PIN
///
#[derive(Debug, UniffiEnum)]
pub enum PinSetErrorReason {
    TooShort,
    TooLong,
    Malformed,
}

impl From<RealPinSetErrorReason> for PinSetErrorReason {
    fn from(value: RealPinSetErrorReason) -> Self {
        match value {
            RealPinSetErrorReason::TooShort => Self::TooShort,
            RealPinSetErrorReason::TooLong => Self::TooLong,
            RealPinSetErrorReason::Malformed => Self::Malformed,
        }
    }
}

/// Specific Reason for error occurrence while authenticating user with PIN
///
#[derive(Debug, UniffiEnum)]
pub enum PinAuthErrorReason {
    TooManyAttempts,
    TooFrequentAttempts,
    IncorrectPin,
}

impl From<RealPinAuthErrorReason> for PinAuthErrorReason {
    fn from(value: RealPinAuthErrorReason) -> Self {
        match value {
            RealPinAuthErrorReason::TooManyAttempts => Self::TooManyAttempts,
            RealPinAuthErrorReason::TooFrequentAttempts => Self::TooFrequentAttempts,
            RealPinAuthErrorReason::IncorrectPin => Self::IncorrectPin,
        }
    }
}

/// Specific Reason for error occurrence within the application.
///
/// This enum is used to represent the specific reason for an error that occurred
/// in handling application related operations in order to provide a way to descirbe
/// common reasons across the application execution errors.
#[derive(Debug, UniffiEnum)]
pub enum OtherErrorReason {
    InvalidParameter,
    Other(String),
}

impl From<RealOtherErrorReason> for OtherErrorReason {
    fn from(reason: RealOtherErrorReason) -> Self {
        match reason {
            RealOtherErrorReason::InvalidParameter => OtherErrorReason::InvalidParameter,
            RealOtherErrorReason::Other(reason) => OtherErrorReason::Other(reason),
        }
    }
}
