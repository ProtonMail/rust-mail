use super::{ActionErrorReason, ProtonError};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::MailContextError;
use proton_mail_common::MailErrorReason as RealMailErrorReason;
use proton_mail_common::ProtonMailError as RealProtonMailError;

#[derive(Debug, From, UniffiEnum)]
pub enum ActionError {
    Reason(ActionErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for ActionError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => ActionError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for ActionError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::ActionReason(reason) => ActionError::Reason(reason.into()),
            other_reason => ActionError::Other(ProtonError::from(other_reason)),
        }
    }
}

impl From<MailContextError> for ActionError {
    fn from(value: MailContextError) -> Self {
        let v = RealProtonMailError::from(value);
        Self::from(v)
    }
}
