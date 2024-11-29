use super::login_flow::HumanChallenge;
use crate::UniffiEnum;
use proton_mail_common::errors::{
    ActionErrorReason as RealActionErrorReason, DraftErrorReason as RealDraftErrorReason,
    EventErrorReason as RealEventErrorReason, LoginErrorReason as RealLoginErrorReason,
    OtherErrorReason as RealOtherErrorReason, SessionErrorReason as RealSessionErrorReason,
};

#[derive(Debug, UniffiEnum)]
pub enum ActionErrorReason {
    UnknownLabel,
    UnknownMessage,
}

impl From<RealActionErrorReason> for ActionErrorReason {
    fn from(reason: RealActionErrorReason) -> Self {
        match reason {
            RealActionErrorReason::UnknownLabel => ActionErrorReason::UnknownLabel,
            RealActionErrorReason::UnknownMessage => ActionErrorReason::UnknownMessage,
        }
    }
}

#[derive(Debug, UniffiEnum)]
pub enum SessionErrorReason {
    UnknownLabel,
}

impl From<RealSessionErrorReason> for SessionErrorReason {
    fn from(reason: RealSessionErrorReason) -> Self {
        match reason {
            RealSessionErrorReason::UnknownLabel => SessionErrorReason::UnknownLabel,
        }
    }
}

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
