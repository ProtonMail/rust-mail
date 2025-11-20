use super::{ProtonError, SessionReason};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::MailErrorReason as RealMailErrorReason;
use proton_mail_common::ProtonMailError as RealProtonMailError;

#[derive(Debug, From, UniffiEnum)]
pub enum UserSessionError {
    Reason(SessionReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for UserSessionError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => UserSessionError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for UserSessionError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::ContextReason(reason) => UserSessionError::Reason(reason.into()),
            other_reason => UserSessionError::Other(ProtonError::from(other_reason)),
        }
    }
}
