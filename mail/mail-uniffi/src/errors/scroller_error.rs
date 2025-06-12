use super::ProtonError;
use crate::UniffiEnum;
use crate::errors::MailScrollerErrorReason;

use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::MailScrollerErrorReason as RealMailScrollerErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use tracing::error;

#[derive(Debug, From, UniffiEnum)]
pub enum MailScrollerError {
    Reason(MailScrollerErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for MailScrollerError {
    fn from(error: RealProtonMailError) -> Self {
        error!("MailScrollerError from {error:?}");
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => MailScrollerError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for MailScrollerError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::MailScrollerReason(RealMailScrollerErrorReason::Dirty) => {
                MailScrollerError::Reason(MailScrollerErrorReason::Dirty)
            }
            other_reason => MailScrollerError::Other(ProtonError::from(other_reason)),
        }
    }
}
