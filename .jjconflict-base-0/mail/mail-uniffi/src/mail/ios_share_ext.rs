use crate::errors::{OtherErrorReason, ProtonError};
use proton_mail_common::{self as ext, IosShareExtension};
use std::path::PathBuf;
use tracing::{instrument, warn};

#[derive(uniffi::Record)]
pub struct IosShareExtDraft {
    pub subject: Option<String>,
    pub body: Option<String>,
    pub inline_attachments: Vec<IosShareExtAttachment>,
    pub attachments: Vec<IosShareExtAttachment>,
}

impl From<IosShareExtDraft> for ext::IosShareExtDraft {
    fn from(value: IosShareExtDraft) -> Self {
        ext::IosShareExtDraft {
            subject: value.subject,
            body: value.body,
            inline_attachments: value
                .inline_attachments
                .into_iter()
                .map(Into::into)
                .collect(),
            attachments: value.attachments.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(uniffi::Record)]
pub struct IosShareExtAttachment {
    pub path: String,
    pub name: Option<String>,
}

impl From<IosShareExtAttachment> for ext::IosShareExtAttachment {
    fn from(value: IosShareExtAttachment) -> Self {
        ext::IosShareExtAttachment {
            path: value.path.into(),
            name: value.name,
        }
    }
}

#[uniffi_export]
#[instrument(skip_all)]
pub fn ios_share_ext_init_draft(mail_cache_dir: String) -> Result<String, ProtonError> {
    let mail_cache_path = PathBuf::from(mail_cache_dir).join("mail-cache");

    let atts_dir = IosShareExtension::init_draft(&mail_cache_path)
        .map_err(|err| ProtonError::OtherReason(OtherErrorReason::Other(err.to_string())))?;

    Ok(atts_dir.display().to_string())
}

#[uniffi_export]
#[instrument(skip_all)]
pub fn ios_share_ext_save_draft(
    mail_cache_dir: String,
    draft: IosShareExtDraft,
) -> Result<(), ProtonError> {
    let mail_cache_path = PathBuf::from(mail_cache_dir).join("mail-cache");

    IosShareExtension::save_draft(&mail_cache_path, draft.into())
        .map_err(|err| ProtonError::OtherReason(OtherErrorReason::Other(err.to_string())))
}
