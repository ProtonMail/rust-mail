use proton_mail_common::errors::update_event::Reason as RealReason;
use proton_mail_common::errors::update_event::UpdateEventError as RealUpdateEventError;

use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::export_void_result;

export_void_result!(VoidUpdateEventResult, UpdateEventError);

#[derive(Debug, uniffi::Enum)]
pub enum UpdateEventError {
    Reason(UpdateEventReason),
    SessionExpired,
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl From<RealUpdateEventError> for UpdateEventError {
    fn from(error: RealUpdateEventError) -> Self {
        match error {
            RealUpdateEventError::Reason(reason) => Self::Reason(UpdateEventReason::from(reason)),
            RealUpdateEventError::SessionExpired => Self::SessionExpired,
            RealUpdateEventError::ServerError(api_service_error) => {
                Self::ServerError(UserApiServiceError::from(api_service_error))
            }
            RealUpdateEventError::Network => Self::Network,
            RealUpdateEventError::Unexpected(unexpected) => {
                Self::Unexpected(UnexpectedError::from(unexpected))
            }
        }
    }
}

/// Reason for invalid Action
#[derive(Debug, uniffi::Enum)]
pub enum UpdateEventReason {
    UnknownLabel,
}

impl From<RealReason> for UpdateEventReason {
    fn from(reason: RealReason) -> Self {
        match reason {
            RealReason::UnknownLabel => Self::UnknownLabel,
        }
    }
}
