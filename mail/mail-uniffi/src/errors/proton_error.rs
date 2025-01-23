use crate::core::datatypes::Id;
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::OtherErrorReason;
use crate::{export_typed_result, export_void_result, UniffiEnum};
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use tracing::error;

#[derive(Debug, UniffiEnum)]
pub enum ProtonError {
    OtherReason(OtherErrorReason),
    SessionExpired,
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl From<RealProtonMailError> for ProtonError {
    fn from(error: RealProtonMailError) -> Self {
        error!("ProtonError from {error:?}");
        match error {
            RealProtonMailError::SessionExpired => ProtonError::SessionExpired,
            RealProtonMailError::ServerError(err) => ProtonError::ServerError(err.into()),
            RealProtonMailError::Network => ProtonError::Network,
            RealProtonMailError::Unexpected(err) => ProtonError::Unexpected(err.into()),
            RealProtonMailError::Reason(reason) => ProtonError::from(reason),
        }
    }
}

impl From<RealMailErrorReason> for ProtonError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::OtherReason(reason) => ProtonError::OtherReason(reason.into()),
            reason => {
                tracing::error!(
                    "Reason mapping failed, this is serious bug, expected OtherErrorReason: {:?}",
                    reason
                );

                ProtonError::Unexpected(UnexpectedError::ErrorMapping)
            }
        }
    }
}

export_void_result!(VoidProtonResult, ProtonError);
export_typed_result!(OptIdProtonResult, Option<Id>, ProtonError);
