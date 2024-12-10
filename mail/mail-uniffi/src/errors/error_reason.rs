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
}

impl From<RealContextErrorReason> for SessionErrorReason {
    fn from(reason: RealContextErrorReason) -> Self {
        match reason {
            RealContextErrorReason::UnknownLabel => SessionErrorReason::UnknownLabel,
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
    UnknownMimeType,
}

impl From<RealDraftErrorReason> for DraftErrorReason {
    fn from(reason: RealDraftErrorReason) -> Self {
        match reason {
            RealDraftErrorReason::UnknownMimeType => DraftErrorReason::UnknownMimeType,
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
