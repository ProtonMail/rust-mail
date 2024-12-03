use super::{ProtonError, SessionErrorReason};
use crate::export_void_result;
use crate::UniffiEnum;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

export_void_result!(VoidSessionResult, SessionError);

#[derive(Debug, UniffiEnum)]
pub enum SessionError {
    Reason(SessionErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for SessionError {
    fn from(error: RealProtonMailError) -> Self {
        {
            match error {
                RealProtonMailError::Reason(reason) => reason.into(),
                mail_error => SessionError::Other(ProtonError::from(mail_error)),
            }
        }
    }
}

impl From<RealMailErrorReason> for SessionError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::SessionReason(reason) => SessionError::Reason(reason.into()),
            other_reason => SessionError::Other(ProtonError::from(other_reason)),
        }
    }
}
