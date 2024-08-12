use crate::mail::MailboxError;
use proton_mail_common::{
    datatypes::attachment::{MimeType as RealMimeType, MimeTypeCategory as RealMimeTypeCategory},
    AppError,
};
use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, UniffiEnum, Hash)]
pub enum MimeTypeCategory {
    Audio,
    Calendar,
    Code,
    Compressed,
    Default,
    Excel,
    Font,
    Image,
    Key,
    Keynote,
    Numbers,
    Pages,
    Pdf,
    Powerpoint,
    Text,
    Video,
    Word,
    Unknown,
}

impl From<RealMimeTypeCategory> for MimeTypeCategory {
    fn from(category: RealMimeTypeCategory) -> Self {
        match category {
            RealMimeTypeCategory::Audio => MimeTypeCategory::Audio,
            RealMimeTypeCategory::Calendar => MimeTypeCategory::Calendar,
            RealMimeTypeCategory::Code => MimeTypeCategory::Code,
            RealMimeTypeCategory::Compressed => MimeTypeCategory::Compressed,
            RealMimeTypeCategory::Default => MimeTypeCategory::Default,
            RealMimeTypeCategory::Excel => MimeTypeCategory::Excel,
            RealMimeTypeCategory::Font => MimeTypeCategory::Font,
            RealMimeTypeCategory::Image => MimeTypeCategory::Image,
            RealMimeTypeCategory::Key => MimeTypeCategory::Key,
            RealMimeTypeCategory::Keynote => MimeTypeCategory::Keynote,
            RealMimeTypeCategory::Numbers => MimeTypeCategory::Numbers,
            RealMimeTypeCategory::Pages => MimeTypeCategory::Pages,
            RealMimeTypeCategory::Pdf => MimeTypeCategory::Pdf,
            RealMimeTypeCategory::Powerpoint => MimeTypeCategory::Powerpoint,
            RealMimeTypeCategory::Text => MimeTypeCategory::Text,
            RealMimeTypeCategory::Video => MimeTypeCategory::Video,
            RealMimeTypeCategory::Word => MimeTypeCategory::Word,
            RealMimeTypeCategory::Unknown => MimeTypeCategory::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, UniffiRecord)]
pub struct MimeType {
    pub mime: String,
    pub category: MimeTypeCategory,
}

impl From<RealMimeType> for MimeType {
    fn from(mime: RealMimeType) -> Self {
        MimeType {
            mime: mime.to_string(),
            category: mime.category().into(),
        }
    }
}

impl TryFrom<MimeType> for RealMimeType {
    type Error = MailboxError;

    fn try_from(mime: MimeType) -> Result<Self, Self::Error> {
        Ok(RealMimeType::new(mime.mime).map_err(AppError::from)?)
    }
}
