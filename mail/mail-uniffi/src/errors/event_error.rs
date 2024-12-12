use super::{EventErrorReason, ProtonError};
use crate::export_void_result;
use crate::UniffiEnum;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

export_void_result!(VoidEventResult, EventError);

#[derive(Debug, UniffiEnum)]
pub enum EventError {
    Reason(EventErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for EventError {
    fn from(error: RealProtonMailError) -> Self {
        {
            match error {
                RealProtonMailError::Reason(reason) => reason.into(),
                mail_error => EventError::Other(ProtonError::from(mail_error)),
            }
        }
    }
}

impl From<RealMailErrorReason> for EventError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::EventReason(reason) => EventError::Reason(reason.into()),
            other_reason => EventError::Other(ProtonError::from(other_reason)),
        }
    }
}
