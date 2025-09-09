use super::{
    DraftAttachmentUploadErrorReason, DraftCancelScheduleSendErrorReason, DraftDiscardErrorReason,
    DraftExpirationErrorReason, DraftOpenErrorReason, DraftPasswordErrorReason,
    DraftSaveErrorReason, DraftSendErrorReason, DraftSenderAddressChangeErrorReason,
    DraftUndoSendErrorReason, ProtonError,
};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::{
    DraftAttachmentUploadErrorReason as RealDraftAttachmentErrorReason, MailErrorReason,
};

#[derive(Debug, From, UniffiEnum)]
pub enum DraftSaveError {
    Reason(DraftSaveErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftSaveError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => DraftSaveError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for DraftSaveError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftSaveReason(reason) => DraftSaveError::Reason(reason.into()),
            // Intercept some of the draft attachment upload validation errors that can be triggered when queuing a save action
            RealMailErrorReason::DraftAttachmentUploadReason(reason) => match reason {
                RealDraftAttachmentErrorReason::TooManyAttachments => {
                    DraftSaveError::Reason(DraftSaveErrorReason::TooManyAttachments)
                }
                RealDraftAttachmentErrorReason::AttachmentTooLarge => {
                    DraftSaveError::Reason(DraftSaveErrorReason::AttachmentTooLarge)
                }
                RealDraftAttachmentErrorReason::TotalAttachmentSizeTooLarge => {
                    DraftSaveError::Reason(DraftSaveErrorReason::TotalAttachmentSizeTooLarge)
                }
                e => DraftSaveError::Other(ProtonError::from(
                    MailErrorReason::DraftAttachmentUploadReason(e),
                )),
            },
            other_reason => DraftSaveError::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftSendError {
    Reason(DraftSendErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftSendError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => DraftSendError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for DraftSendError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftSendReason(reason) => DraftSendError::Reason(reason.into()),
            other_reason => DraftSendError::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftOpenError {
    Reason(DraftOpenErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftOpenError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => DraftOpenError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for DraftOpenError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftOpenReason(reason) => DraftOpenError::Reason(reason.into()),
            other_reason => DraftOpenError::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftUndoSendError {
    Reason(DraftUndoSendErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftUndoSendError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => DraftUndoSendError::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for DraftUndoSendError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftUndoSendReason(reason) => Self::Reason(reason.into()),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftDiscardError {
    Reason(DraftDiscardErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftDiscardError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => Self::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for DraftDiscardError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftDiscardReason(reason) => Self::Reason(reason.into()),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftAttachmentUploadError {
    Reason(DraftAttachmentUploadErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftAttachmentUploadError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

impl From<RealMailErrorReason> for DraftAttachmentUploadError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftAttachmentUploadReason(reason) => Self::Reason(reason.into()),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}
#[derive(Debug, From, UniffiEnum)]
pub enum DraftCancelScheduleSendError {
    Reason(DraftCancelScheduleSendErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftCancelScheduleSendError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

impl From<RealMailErrorReason> for DraftCancelScheduleSendError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftCancelScheduleSendReason(reason) => {
                Self::Reason(reason.into())
            }
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftSenderAddressChangeError {
    Reason(DraftSenderAddressChangeErrorReason),
    Other(ProtonError),
}

impl From<RealMailErrorReason> for DraftSenderAddressChangeError {
    fn from(value: RealMailErrorReason) -> Self {
        match value {
            RealMailErrorReason::DraftSenderAddressChangeReason(reason) => {
                Self::Reason(reason.into())
            }
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

impl From<RealProtonMailError> for DraftSenderAddressChangeError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftPasswordError {
    Reason(DraftPasswordErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftPasswordError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => Self::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for DraftPasswordError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftPasswordReason(reason) => Self::Reason(reason.into()),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}

#[derive(Debug, From, UniffiEnum)]
pub enum DraftExpirationError {
    Reason(DraftExpirationErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftExpirationError {
    fn from(error: RealProtonMailError) -> Self {
        match error {
            RealProtonMailError::Reason(reason) => reason.into(),
            mail_error => Self::Other(ProtonError::from(mail_error)),
        }
    }
}

impl From<RealMailErrorReason> for DraftExpirationError {
    fn from(reason: RealMailErrorReason) -> Self {
        match reason {
            RealMailErrorReason::DraftExpirationReason(reason) => Self::Reason(reason.into()),
            other_reason => Self::Other(ProtonError::from(other_reason)),
        }
    }
}
