use crate::UniffiEnum;
use crate::errors::OtherErrorReason;
use crate::errors::api_service_error::UserApiServiceError;
use crate::errors::unexpected::UnexpectedError;
use crate::mail::initialization::MailUserSessionInitializationStage;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use tracing::error;

#[derive(Debug, From, UniffiEnum)]
pub enum ProtonError {
    OtherReason(OtherErrorReason),
    SessionExpired,
    ServerError(UserApiServiceError),
    Network,
    /// Initialization failed, possibly because of lack of network. App is completely unusable until fixed.
    Initialization(MailUserSessionInitializationStage),
    Unexpected(UnexpectedError),
}

impl From<RealProtonMailError> for ProtonError {
    fn from(error: RealProtonMailError) -> Self {
        error!("ProtonError from {error:?}");
        match error {
            RealProtonMailError::SessionExpired => ProtonError::SessionExpired,
            RealProtonMailError::ServerError(err) => ProtonError::ServerError(err.into()),
            RealProtonMailError::Network => ProtonError::Network,
            RealProtonMailError::InitializationFailed(stage) => {
                ProtonError::Initialization(stage.into())
            }
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
