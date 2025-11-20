use super::ProtonError;
use crate::UniffiEnum;
use crate::errors::SnoozeErrorReason;

use derive_more::From;
use proton_mail_common::MailErrorReason as RealMailErrorReason;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::SnoozeErrorReason as RealSnoozeErrorReason;

#[derive(Debug, From, UniffiEnum)]
pub enum SnoozeError {
    Reason(SnoozeErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for SnoozeError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => SnoozeError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for SnoozeError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::SnoozeReason(RealSnoozeErrorReason::SnoozeTimeInThePast) => {
                SnoozeError::Reason(SnoozeErrorReason::SnoozeTimeInThePast)
            }
            RealMailErrorReason::SnoozeReason(RealSnoozeErrorReason::InvalidSnoozeLocation) => {
                SnoozeError::Reason(SnoozeErrorReason::InvalidSnoozeLocation)
            }
            other_reason => SnoozeError::Other(ProtonError::from(other_reason)),
        }
    }
}
