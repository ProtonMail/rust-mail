use proton_mail_common::errors::MailErrorDetails as RealMailErrorDetails;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::Reason as RealReason;

use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::export_void_result;

use super::login_flow::HumanChallenge;

export_void_result!(VoidProtonMailResult, ProtonMailError);

#[derive(Debug, uniffi::Record)]
pub struct ProtonMailError {
    pub kind: MailErrorKind,
    pub details: MailErrorDetails,
}

#[derive(Copy, Clone, Debug, uniffi::Enum)]
pub enum MailErrorKind {
    UserActionError,
    UserSessionError,
    UserDraftError,
    LoginFlowError,
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

#[derive(Debug, uniffi::Enum)]
pub enum MailErrorDetails {
    Reason(Reason),
    SessionExpired,
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl<I: Into<RealMailErrorDetails>> From<I> for MailErrorDetails {
    fn from(error: I) -> Self {
        let error = error.into();
        match error {
            RealMailErrorDetails::Reason(reason) => Self::Reason(Reason::from(reason)),
            RealMailErrorDetails::SessionExpired => Self::SessionExpired,
            RealMailErrorDetails::ServerError(user_api_service_error) => {
                Self::ServerError(UserApiServiceError::from(user_api_service_error))
            }
            RealMailErrorDetails::Network => Self::Network,
            RealMailErrorDetails::Unexpected(unexpected) => {
                Self::Unexpected(UnexpectedError::from(unexpected))
            }
        }
    }
}

impl From<Reason> for MailErrorDetails {
    fn from(reason: Reason) -> Self {
        Self::Reason(reason)
    }
}

/// Reason for invalid Action
#[derive(Debug, uniffi::Enum)]
pub enum Reason {
    InvalidParameter,
    UnknownLabel,
    UnknownMessage,
    HumanVerificationChallenge(HumanChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

impl From<RealReason> for Reason {
    fn from(value: RealReason) -> Self {
        match value {
            RealReason::InvalidParameter => Self::InvalidParameter,
            RealReason::UnknownLabel => Self::UnknownLabel,
            RealReason::UnknownMessage => Self::UnknownMessage,
            RealReason::HumanVerificationChallenge(challenge) => {
                Self::HumanVerificationChallenge(challenge.into())
            }
            RealReason::InvalidCredentials => Self::InvalidCredentials,
            RealReason::UnsupportedTfa => Self::UnsupportedTfa,
            RealReason::CantUnlockUserKey => Self::CantUnlockUserKey,
        }
    }
}
