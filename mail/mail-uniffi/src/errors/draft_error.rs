use super::{
    DraftAttachmentUploadErrorReason, DraftDiscardErrorReason, DraftOpenErrorReason,
    DraftSaveErrorReason, DraftSendErrorReason, DraftUndoSendErrorReason, ProtonError,
};
use crate::UniffiEnum;
use derive_more::From;
use proton_mail_common::errors::MailErrorReason as RealMailErrorReason;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use tracing::error;

#[derive(Debug, From, UniffiEnum)]
pub enum DraftSaveError {
    Reason(DraftSaveErrorReason),
    Other(ProtonError),
}

impl From<RealProtonMailError> for DraftSaveError {
    fn from(error: RealProtonMailError) -> Self {
        error!("DraftSendError from {error:?}");
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
        error!("DraftSendError from {error:?}");
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
        error!("DraftOpenError from {error:?}");
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
        error!("DraftUndoSendError from {error:?}");
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
        error!("DraftDiscardError from {error:?}");
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
        error!("DraftDiscardError from {error:?}");
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
