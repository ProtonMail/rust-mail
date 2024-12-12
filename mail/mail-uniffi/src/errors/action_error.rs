use super::{ActionErrorReason, ProtonError};
use crate::export_void_result;
use crate::UniffiEnum;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

export_void_result!(VoidActionResult, ActionError);

#[derive(Debug, UniffiEnum)]
pub enum ActionError {
    Reason(ActionErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for ActionError {
    fn from(error: RealProtonMailError) -> Self {
        {
            match error {
                RealProtonMailError::Reason(reason) => reason.into(),
                mail_error => ActionError::Other(ProtonError::from(mail_error)),
            }
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
