use proton_mail_common::errors::user_session::Reason as RealReason;
use proton_mail_common::errors::user_session::UserSessionError as RealUserSessionError;

use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::export_void_result;

export_void_result!(VoidUserSessionResult, UserSessionError);

#[derive(Debug, uniffi::Enum)]
pub enum UserSessionError {
    Reason(SessionReason),
    SessionExpired,
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl From<RealUserSessionError> for UserSessionError {
    fn from(error: RealUserSessionError) -> Self {
        match error {
            RealUserSessionError::Reason(reason) => Self::Reason(SessionReason::from(reason)),
            RealUserSessionError::SessionExpired => Self::SessionExpired,
            RealUserSessionError::ServerError(user_api_service_error) => {
                Self::ServerError(UserApiServiceError::from(user_api_service_error))
            }
            RealUserSessionError::Network => Self::Network,
            RealUserSessionError::Unexpected(unexpected) => {
                Self::Unexpected(UnexpectedError::from(unexpected))
            }
        }
    }
}

/// Reason for invalid Action
#[derive(Debug, uniffi::Enum)]
pub enum SessionReason {
    UnknownLabel,
}

impl From<RealReason> for SessionReason {
    fn from(reason: RealReason) -> Self {
        match reason {
            RealReason::UnknownLabel => Self::UnknownLabel,
        }
    }
}
