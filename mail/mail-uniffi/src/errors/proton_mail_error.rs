use proton_mail_common::errors::MailErrorDetails as RealMailErrorDetails;
use proton_mail_common::errors::MailErrorReason as RealReason;

use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::export_void_result;

use super::login_flow::HumanChallenge;

export_void_result!(VoidProtonMailResult, ProtonMailError);

/// Represent all the errors that can be returned by the ProtonMail SDK.
#[derive(Debug, uniffi::Record)]
pub struct ProtonMailError {
    pub kind: MailErrorKind,
    pub details: MailErrorDetails,
}

/// Possible Mail Localizable Errors
#[derive(Copy, Clone, Debug, uniffi::Enum)]
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

#[derive(Debug, uniffi::Enum)]
pub enum MailErrorDetails {
    /// This error detail is related with the arguments (i.e. like a Message id who does not exist)
    Reason(MailErrorReason),
    /// This error detail is used when the session is expired.
    SessionExpired,
    /// This error detail come from the Backend (i.e. like a 404 error)
    ServerError(UserApiServiceError),
    /// This error detail come form network (i.e. like can't connect to backend)
    Network,
    /// Something unexpected happened
    Unexpected(UnexpectedError),
}

impl<I: Into<RealMailErrorDetails>> From<I> for MailErrorDetails {
    fn from(error: I) -> Self {
        let error = error.into();
        match error {
            RealMailErrorDetails::Reason(reason) => Self::Reason(MailErrorReason::from(reason)),
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

/// Specific Reason for error occurrence
#[derive(Debug, uniffi::Enum)]
pub enum MailErrorReason {
    InvalidParameter,
    UnknownLabel,
    UnknownMessage,
    HumanVerificationChallenge(HumanChallenge),
    InvalidCredentials,
    UnsupportedTfa,
    CantUnlockUserKey,
}

impl From<MailErrorReason> for MailErrorDetails {
    fn from(reason: MailErrorReason) -> Self {
        Self::Reason(reason)
    }
}

impl From<RealReason> for MailErrorReason {
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
