use super::login_flow::HumanChallenge;
use crate::UniffiEnum;
use proton_mail_common::errors::{
    ActionErrorReason as RealActionErrorReason, ContextErrorReason as RealContextErrorReason,
    DraftErrorReason as RealDraftErrorReason, EventErrorReason as RealEventErrorReason,
    LoginErrorReason as RealLoginErrorReason, OtherErrorReason as RealOtherErrorReason,
};

/// Specific Reason for error occurrence within ActionQueue
///
/// This enum is used to represent the specific reason for an error that occurred
/// in oreder to provide only the necessary information to the user.
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
/// as context is nomeclature used in the common library.
#[derive(Debug, UniffiEnum)]
pub enum SessionErrorReason {
    UnknownLabel,
    DuplicateContext,
}

impl From<RealContextErrorReason> for SessionErrorReason {
    fn from(reason: RealContextErrorReason) -> Self {
        match reason {
            RealContextErrorReason::UnknownLabel => SessionErrorReason::UnknownLabel,
            RealContextErrorReason::DuplicateContext => SessionErrorReason::DuplicateContext,
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
    HumanVerificationChallenge(HumanChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

impl From<RealLoginErrorReason> for LoginErrorReason {
    fn from(reason: RealLoginErrorReason) -> Self {
        match reason {
            RealLoginErrorReason::HumanVerificationChallenge(challenge) => {
                LoginErrorReason::HumanVerificationChallenge(challenge.into())
            }
            RealLoginErrorReason::InvalidCredentials => LoginErrorReason::InvalidCredentials,
            RealLoginErrorReason::UnsupportedTfa => LoginErrorReason::UnsupportedTfa,
            RealLoginErrorReason::CantUnlockUserKey => LoginErrorReason::CantUnlockUserKey,
        }
    }
}

/// Specific Reason for error occurrence within Draft.
///
/// This enum is used to represent the specific reason for an error that occurred
/// while drafting a new message in order to provide only the necessary
/// information to the user.
#[derive(Debug, UniffiEnum)]
pub enum DraftErrorReason {
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
    /// Updating a message that is not draft.
    MessageUpdateIsNotDraft,
    /// This message no longer exists.
    MessageDoesNotExist,
    /// This draft was already sent and can't be modified
    AlreadySent,
    /// Can not undo sent this message
    MessageCanNotBeUndoSent,
    /// The cancellation of sending for this message is no longer possible.
    SendCanNoLongerBeUndone,
}

impl From<RealDraftErrorReason> for DraftErrorReason {
    fn from(reason: RealDraftErrorReason) -> Self {
        match reason {
            RealDraftErrorReason::NoRecipients => Self::NoRecipients,
            RealDraftErrorReason::AddressDoesNotHavePrimaryKey(v) => {
                Self::AddressDoesNotHavePrimaryKey(v.into_inner())
            }
            RealDraftErrorReason::RecipientEmailInvalid(v) => Self::RecipientEmailInvalid(v),
            RealDraftErrorReason::ProtonRecipientDoesNotExist(v) => {
                Self::ProtonRecipientDoesNotExist(v)
            }
            RealDraftErrorReason::UnknownRecipientValidationError(v) => {
                Self::UnknownRecipientValidationError(v)
            }
            RealDraftErrorReason::AddressDisabled(v) => Self::AddressDisabled(v),
            RealDraftErrorReason::MessageAlreadySent => Self::MessageAlreadySent,
            RealDraftErrorReason::PackageError(v) => Self::PackageError(v),
            RealDraftErrorReason::MessageUpdateIsNotDraft => Self::MessageUpdateIsNotDraft,
            RealDraftErrorReason::MessageDoesNotExist => Self::MessageDoesNotExist,
            RealDraftErrorReason::AlreadySent => Self::AlreadySent,
            RealDraftErrorReason::MessageCanNotBeUndoSent => Self::MessageCanNotBeUndoSent,
            RealDraftErrorReason::SendCanNoLongerBeUndone => Self::SendCanNoLongerBeUndone,
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
    Placeholder,
}

impl From<RealEventErrorReason> for EventErrorReason {
    fn from(reason: RealEventErrorReason) -> Self {
        match reason {
            RealEventErrorReason::Placeholder => EventErrorReason::Placeholder,
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
