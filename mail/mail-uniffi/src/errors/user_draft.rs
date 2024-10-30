use proton_mail_common::errors::user_draft::Reason as RealReason;
use proton_mail_common::errors::user_draft::UserDraftError as RealUserDraftError;

use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::export_void_result;

export_void_result!(VoidUserDraftResult, UserDraftError);

#[derive(Debug, uniffi::Enum)]
pub enum UserDraftError {
    Reason(DraftReason),
    SessionExpired,
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl From<RealUserDraftError> for UserDraftError {
    fn from(error: RealUserDraftError) -> Self {
        match error {
            RealUserDraftError::Reason(reason) => Self::Reason(DraftReason::from(reason)),
            RealUserDraftError::SessionExpired => Self::SessionExpired,
            RealUserDraftError::ServerError(user_api_service_error) => {
                Self::ServerError(UserApiServiceError::from(user_api_service_error))
            }
            RealUserDraftError::Network => Self::Network,
            RealUserDraftError::Unexpected(unexpected) => {
                Self::Unexpected(UnexpectedError::from(unexpected))
            }
        }
    }
}

/// Reason for invalid Action
#[derive(Debug, uniffi::Enum)]
pub enum DraftReason {
    UnknownLabel,
}

impl From<RealReason> for DraftReason {
    fn from(reason: RealReason) -> Self {
        match reason {
            RealReason::UnknownLabel => Self::UnknownLabel,
        }
    }
}
