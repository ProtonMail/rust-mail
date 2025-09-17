use crate::{UniffiEnum, mail::Origin};
use proton_mail_common::errors::{
    ActionErrorReason as RealActionErrorReason, ContextErrorReason as RealContextErrorReason,
    DraftAttachmentUploadErrorReason as RealDraftAttachmentErrorReason,
    DraftCancelScheduleSendErrorReason as RealDraftCancelScheduleSendErrorReason,
    DraftDiscardErrorReason as RealDraftDiscardErrorReason,
    DraftExpirationErrorReason as RealDraftExpirationErrorReason,
    DraftOpenErrorReason as RealDraftOpenErrorReason,
    DraftPasswordErrorReason as RealDraftPasswordErrorReason,
    DraftSaveErrorReason as RealDraftSaveErrorReason,
    DraftSendErrorReason as RealDraftSendErrorReason,
    DraftSenderAddressChangeErrorReason as RealDraftSenderAddressChangeErrorReason,
    DraftUndoSendErrorReason as RealDraftUndoSendErrorReason,
    EventErrorReason as RealEventErrorReason,
    MailScrollerErrorReason as RealMailScrollerErrorReason,
    OtherErrorReason as RealOtherErrorReason, PinAuthErrorReason as RealPinAuthErrorReason,
    PinSetErrorReason as RealPinSetErrorReason, SnoozeErrorReason as RealSnoozeErrorReason,
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
pub enum SessionReason {
    UnknownLabel,
    DuplicateSession,
    UserSessionNotInitialized,
    /// Mobile dev used a method that is supported only in one origin.
    /// Example: Method that can be called only in the main application process, was called from iOS share extension.
    MethodCalledInWrongOrigin {
        expected: Origin,
        actual: Origin,
    },
}

impl From<RealContextErrorReason> for SessionReason {
    fn from(reason: RealContextErrorReason) -> Self {
        match reason {
            RealContextErrorReason::DuplicateContext => SessionReason::DuplicateSession,
            RealContextErrorReason::UserContextNotInitialized(_) => {
                SessionReason::UserSessionNotInitialized
            }
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
    /// This address is disabled and can't be used for sending
    AddressDisabled(String),
    /// Message was already sent.
    MessageAlreadySent,
    /// This message no longer exists.
    MessageDoesNotExist,
    /// Message is not a draft
    MessageIsNotADraft,
    /// Too Many Attachments
    TooManyAttachments,
    /// The attachment size exceeds the upper limit
    AttachmentTooLarge,
    /// The combined attachment size exceeds the upper limit
    TotalAttachmentSizeTooLarge,
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
    /// Time at which the message was scheduled to send has already expired
    ScheduleSendExpired,
    /// The maximum number of scheduled send messages has been reached.
    ScheduleSendMessageLimitExceeded,
    /// Failed to decrypt external encryption password
    EOPasswordDecrypt,
    /// Expiration time is too soon
    ExpirationTimeTooSoon,
    /// Message + Attachment size too large
    MessageTooLarge,
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
            RealDraftSaveErrorReason::AddressDisabled(value) => Self::AddressDisabled(value),
            RealDraftSaveErrorReason::MessageAlreadySent => Self::MessageAlreadySent,
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
                Self::RecipientEmailInvalid(value.into_clear_text_string())
            }
            RealDraftSendErrorReason::ProtonRecipientDoesNotExist(value) => {
                Self::ProtonRecipientDoesNotExist(value.into_clear_text_string())
            }
            RealDraftSendErrorReason::PackageError(value) => Self::PackageError(value),
            RealDraftSendErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
            RealDraftSendErrorReason::MessageIsNotADraft => Self::MessageIsNotADraft,
            RealDraftSendErrorReason::MissingAttachmentUploads => Self::MissingAttachmentUploads,
            RealDraftSendErrorReason::ScheduleSendExpired => Self::ScheduleSendExpired,
            RealDraftSendErrorReason::ScheduleSendMessageLimitExceeded => {
                Self::ScheduleSendMessageLimitExceeded
            }
            RealDraftSendErrorReason::EOPasswordDecrypt => Self::EOPasswordDecrypt,
            RealDraftSendErrorReason::ExpirationTimeTooSoon => Self::ExpirationTimeTooSoon,
            RealDraftSendErrorReason::MessageTooLarge => Self::MessageTooLarge,
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
    /// This message no longer exists.
    MessageDoesNotExist,
}

impl From<RealDraftUndoSendErrorReason> for DraftUndoSendErrorReason {
    fn from(value: RealDraftUndoSendErrorReason) -> Self {
        match value {
            RealDraftUndoSendErrorReason::MessageCanNotBeUndoSent => Self::MessageCanNotBeUndoSent,
            RealDraftUndoSendErrorReason::SendCanNoLongerBeUndone => Self::SendCanNoLongerBeUndone,
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
    /// Combined Attachment size is too large
    TotalAttachmentSizeTooLarge,
    /// Attachment upload timed out
    Timeout,
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
            RealDraftAttachmentErrorReason::TotalAttachmentSizeTooLarge => {
                Self::TotalAttachmentSizeTooLarge
            }
            RealDraftAttachmentErrorReason::RetryInvalidState => Self::RetryInvalidState,
            RealDraftAttachmentErrorReason::Timeout => Self::Timeout,
        }
    }
}

#[derive(Debug, UniffiEnum)]
pub enum DraftCancelScheduleSendErrorReason {
    MessageDoesNotExist,
    MessageNotScheduled,
    MessageAlreadySent,
}

impl From<RealDraftCancelScheduleSendErrorReason> for DraftCancelScheduleSendErrorReason {
    fn from(value: RealDraftCancelScheduleSendErrorReason) -> Self {
        match value {
            RealDraftCancelScheduleSendErrorReason::MessageDoesNotExist => {
                Self::MessageDoesNotExist
            }
            RealDraftCancelScheduleSendErrorReason::MessageNotScheduled => {
                Self::MessageNotScheduled
            }
            RealDraftCancelScheduleSendErrorReason::MessageAlreadySent => Self::MessageAlreadySent,
        }
    }
}

#[derive(Debug, UniffiEnum)]
pub enum DraftSenderAddressChangeErrorReason {
    AddressEmailNotFound(String),
    AddressNotSendEnabled,
    AddressDisabled,
}

impl From<RealDraftSenderAddressChangeErrorReason> for DraftSenderAddressChangeErrorReason {
    fn from(value: RealDraftSenderAddressChangeErrorReason) -> Self {
        match value {
            RealDraftSenderAddressChangeErrorReason::AddressNotSendEnabled => {
                Self::AddressNotSendEnabled
            }
            RealDraftSenderAddressChangeErrorReason::AddressDisabled => Self::AddressDisabled,
            RealDraftSenderAddressChangeErrorReason::AddressWithEmailNotFound(v) => {
                Self::AddressEmailNotFound(v)
            }
        }
    }
}

#[derive(Debug, UniffiEnum)]
pub enum DraftPasswordErrorReason {
    PasswordTooShort,
}

impl From<RealDraftPasswordErrorReason> for DraftPasswordErrorReason {
    fn from(value: RealDraftPasswordErrorReason) -> Self {
        match value {
            RealDraftPasswordErrorReason::PasswordTooShort => Self::PasswordTooShort,
        }
    }
}

#[derive(Debug, UniffiEnum)]
pub enum DraftExpirationErrorReason {
    ExpirationTimeInThePast,
    ExpirationTimeLessThan15Min,
    ExpirationTimeExceeds30Days,
}

impl From<RealDraftExpirationErrorReason> for DraftExpirationErrorReason {
    fn from(value: RealDraftExpirationErrorReason) -> Self {
        match value {
            RealDraftExpirationErrorReason::ExpirationTimeInThePast => {
                Self::ExpirationTimeInThePast
            }
            RealDraftExpirationErrorReason::ExpirationTimeExceeds28Days => {
                Self::ExpirationTimeExceeds30Days
            }
            RealDraftExpirationErrorReason::ExpirationTimeLessThan15Min => {
                Self::ExpirationTimeLessThan15Min
            }
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

#[derive(Debug, UniffiEnum)]
pub enum MailScrollerErrorReason {
    Dirty,
}

impl From<RealMailScrollerErrorReason> for MailScrollerErrorReason {
    fn from(value: RealMailScrollerErrorReason) -> Self {
        match value {
            RealMailScrollerErrorReason::Dirty => Self::Dirty,
        }
    }
}

#[derive(Debug, UniffiEnum)]
pub enum SnoozeErrorReason {
    SnoozeTimeInThePast,
    InvalidSnoozeLocation,
}

impl From<RealSnoozeErrorReason> for SnoozeErrorReason {
    fn from(value: RealSnoozeErrorReason) -> Self {
        match value {
            RealSnoozeErrorReason::SnoozeTimeInThePast => Self::SnoozeTimeInThePast,
            RealSnoozeErrorReason::InvalidSnoozeLocation => Self::InvalidSnoozeLocation,
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
