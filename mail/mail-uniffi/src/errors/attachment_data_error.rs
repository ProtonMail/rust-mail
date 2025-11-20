use super::ProtonError;
use derive_more::From;
use proton_mail_common::{MailContextError, ProtonMailError as RealProtonMailError};
use tokio::task::JoinError;
use uniffi::Enum;

#[derive(Debug, From, Enum)]
pub enum AttachmentDataError {
    ProxyFailed,
    Other(ProtonError),
}

impl From<MailContextError> for AttachmentDataError {
    fn from(value: MailContextError) -> Self {
        if let MailContextError::ImageProxyFailed = value {
            Self::ProxyFailed
        } else {
            RealProtonMailError::from(value).into()
        }
    }
}

impl From<JoinError> for AttachmentDataError {
    fn from(value: JoinError) -> Self {
        RealProtonMailError::from(value).into()
    }
}

impl From<RealProtonMailError> for AttachmentDataError {
    fn from(value: RealProtonMailError) -> Self {
        Self::Other(value.into())
    }
}
