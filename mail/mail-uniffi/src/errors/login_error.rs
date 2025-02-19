use super::{LoginErrorReason, ProtonError};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

#[derive(Debug, From, UniffiEnum)]
pub enum LoginError {
    Reason(LoginErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for LoginError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => LoginError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for LoginError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::LoginReason(reason) => LoginError::Reason(reason.into()),
            other_reason => LoginError::Other(ProtonError::from(other_reason)),
        }
    }
}
