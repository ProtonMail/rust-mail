use super::{DraftErrorReason, ProtonError};
use crate::core::datatypes::Id;
use crate::UniffiEnum;
use crate::{export_typed_result, export_void_result};
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

export_void_result!(VoidDraftResult, DraftError);
export_typed_result!(OptIdDraftResult, Option<Id>, DraftError);

#[derive(Debug, UniffiEnum)]
pub enum DraftError {
    Reason(DraftErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftError {
    fn from(error: RealProtonMailError) -> Self {
        {
            match error {
                RealProtonMailError::Reason(reason) => reason.into(),
                mail_error => DraftError::Other(ProtonError::from(mail_error)),
            }
        }
    }
}

impl From<RealMailErrorReason> for DraftError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftReason(reason) => DraftError::Reason(reason.into()),
            other_reason => DraftError::Other(ProtonError::from(other_reason)),
        }
    }
}
