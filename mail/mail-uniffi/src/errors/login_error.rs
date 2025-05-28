use super::{LoginErrorReason, ProtonError};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

#[derive(Debug, From, UniffiEnum)]
pub enum MailLoginError {
    Reason(LoginErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for MailLoginError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => MailLoginError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for MailLoginError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::LoginReason(reason) => MailLoginError::Reason(reason.into()),
            other_reason => MailLoginError::Other(ProtonError::from(other_reason)),
        }
    }
}
