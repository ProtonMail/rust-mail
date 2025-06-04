use crate::UniffiEnum;
use crate::errors::OtherErrorReason;
use crate::errors::unexpected::UnexpectedError;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::api_service_error::UserApiServiceError as RealUserApiServiceError;
use tracing::error;
use uniffi_common::errors::UserApiServiceError;

#[derive(Debug, From, UniffiEnum)]
pub enum ProtonError {
    OtherReason(OtherErrorReason),
    ServerError(UserApiServiceError),
    Network,
    Unexpected(UnexpectedError),
}

impl From<RealProtonMailError> for ProtonError {
    fn from(error: RealProtonMailError) -> Self {
        error!("ProtonError from {error:?}");
        match error {
            RealProtonMailError::ServerError(err) => {
                ProtonError::ServerError(into_uniffi_error(err))
            }
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

fn into_uniffi_error(error: RealUserApiServiceError) -> UserApiServiceError {
    error!("UserApiServiceError from {error:?}");
    match error {
        RealUserApiServiceError::BadRequest(text) => UserApiServiceError::BadRequest(text),
        RealUserApiServiceError::Unauthorized(text) => UserApiServiceError::Unauthorized(text),
        RealUserApiServiceError::NotFound(text) => UserApiServiceError::NotFound(text),
        RealUserApiServiceError::UnprocessableEntity(text) => {
            UserApiServiceError::UnprocessableEntity(text)
        }
        RealUserApiServiceError::TooManyRequests(text) => {
            UserApiServiceError::TooManyRequests(text)
        }
        RealUserApiServiceError::InternalServerError(text) => {
            UserApiServiceError::InternalServerError(text)
        }
        RealUserApiServiceError::NotImplemented(text) => UserApiServiceError::NotImplemented(text),
        RealUserApiServiceError::BadGateway(text) => UserApiServiceError::BadGateway(text),
        RealUserApiServiceError::ServiceUnavailable(text) => {
            UserApiServiceError::ServiceUnavailable(text)
        }
        RealUserApiServiceError::OtherHttpError(code, text) => {
            UserApiServiceError::OtherHttpError(code, text)
        }
    }
}
