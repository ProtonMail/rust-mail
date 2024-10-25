use proton_mail_common::errors::user_actions::Reason as RealReason;
use proton_mail_common::errors::user_actions::UserActionError as RealUserActionError;

use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::export_void_result;

export_void_result!(VoidUserActionResult, UserActionError);

#[derive(Debug, uniffi::Enum)]
pub enum UserActionError {
    InvalidAction(ActionReason),
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl From<RealUserActionError> for UserActionError {
    fn from(error: RealUserActionError) -> Self {
        match error {
            RealUserActionError::InvalidAction(reason) => {
                Self::InvalidAction(ActionReason::from(reason))
            }
            RealUserActionError::ServerError(user_api_service_error) => {
                Self::ServerError(UserApiServiceError::from(user_api_service_error))
            }
            RealUserActionError::Network => Self::Network,
            RealUserActionError::Unexpected(unexpected) => {
                Self::Unexpected(UnexpectedError::from(unexpected))
            }
        }
    }
}

impl From<ActionReason> for UserActionError {
    fn from(reason: ActionReason) -> Self {
        Self::InvalidAction(reason)
    }
}

/// Reason for invalid Action
#[derive(Debug, uniffi::Enum)]
pub enum ActionReason {
    InvalidParameter,
    UnknownLabel,
    UnknownMessage,
}

impl From<RealReason> for ActionReason {
    fn from(value: RealReason) -> Self {
        match value {
            RealReason::InvalidParameter => Self::InvalidParameter,
            RealReason::UnknownLabel => Self::UnknownLabel,
            RealReason::UnknownMessage => Self::UnknownMessage,
        }
    }
}
