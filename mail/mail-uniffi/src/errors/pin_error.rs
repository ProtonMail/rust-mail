use super::{PinAuthErrorReason, PinSetErrorReason, ProtonError};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use tracing::error;

#[derive(Debug, From, UniffiEnum)]
pub enum PinSetError {
    Reason(PinSetErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for PinSetError {
    fn from(error: RealProtonMailError) -> Self {
        error!("PinSetError from {error:?}");
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => PinSetError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for PinSetError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::PinSetReason(reason) => PinSetError::Reason(reason.into()),
            other_reason => PinSetError::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum PinAuthError {
    Reason(PinAuthErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for PinAuthError {
    fn from(error: RealProtonMailError) -> Self {
        error!("PinAuthError from {error:?}");
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => PinAuthError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for PinAuthError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::PinAuthReason(reason) => PinAuthError::Reason(reason.into()),
            other_reason => PinAuthError::Other(ProtonError::from(other_reason)),
        }
    }
}
