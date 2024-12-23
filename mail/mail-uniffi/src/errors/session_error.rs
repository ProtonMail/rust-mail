use super::{ProtonError, SessionErrorReason};
use crate::export_void_result;
use crate::UniffiEnum;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use tracing::error;

export_void_result!(VoidSessionResult, UserSessionError);

#[derive(Debug, UniffiEnum)]
pub enum UserSessionError {
    Reason(SessionErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for UserSessionError {
    fn from(error: RealProtonMailError) -> Self {
        error!("UserSessionError from {error:?}");
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => UserSessionError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for UserSessionError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::SessionReason(reason) => UserSessionError::Reason(reason.into()),
            other_reason => UserSessionError::Other(ProtonError::from(other_reason)),
        }
    }
}
