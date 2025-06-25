use super::{ContextReason, ProtonError};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

#[derive(Debug, From, UniffiEnum)]
pub enum UserContextError {
    Reason(ContextReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for UserContextError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => UserContextError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for UserContextError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::ContextReason(reason) => UserContextError::Reason(reason.into()),
            other_reason => UserContextError::Other(ProtonError::from(other_reason)),
        }
    }
}
