#![allow(clippy::result_large_err)]

use crate::MailContextError;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, instrument, warn};

#[derive(Debug)]
pub struct IosShareExtension;

impl IosShareExtension {
    #[instrument]
    pub fn init_draft(mail_cache_path: &Path) -> Result<PathBuf, MailContextError> {
        info!("Initializing share extension's draft");

        let (dir, atts, _) = Self::paths(mail_cache_path);

        if dir.exists() {
            fs::remove_dir_all(&dir)
                .inspect_err(|err| error!(?err, "Couldn't clean temporary directory"))?;
        }

        fs::create_dir(&dir)
            .inspect_err(|err| error!(?err, "Couldn't create temporary directory"))?;

        fs::create_dir(&atts)
            .inspect_err(|err| error!(?err, "Couldn't create temporary attachments directory"))?;

        Ok(atts)
    }

    #[instrument(skip(draft))]
    pub fn save_draft(
        mail_cache_path: &Path,
        draft: IosShareExtDraft,
    ) -> Result<(), MailContextError> {
        info!("Saving share extension's draft");

        let (_, _, path) = Self::paths(mail_cache_path);
        let data = serde_json::to_string(&draft).unwrap();

        fs::write(path, data).inspect_err(|err| error!(?err, "Couldn't write draft"))?;

        Ok(())
    }

    #[instrument]
    pub fn load_draft(
        mail_cache_path: &Path,
    ) -> Result<Option<IosShareExtDraft>, MailContextError> {
        info!("Loading share extension's draft");

        let (dir, _, draft) = Self::paths(mail_cache_path);

        if dir.exists() && draft.exists() {
            let draft = fs::read(draft).inspect_err(|err| error!(?err, "Couldn't load draft"))?;

            let draft = serde_json::from_slice(&draft)
                .inspect_err(|err| error!(?err, "Couldn't deserialize draft"))
                .map_err(|_| MailContextError::Other(anyhow!("Couldn't deserialize draft")))?;

            Ok(Some(draft))
        } else {
            Ok(None)
        }
    }

    #[instrument]
    pub fn delete_draft(mail_cache_path: &Path) {
        info!("Clearing share extension's draft");

        let (dir, _, _) = Self::paths(mail_cache_path);

        if dir.exists() {
            _ = fs::remove_dir_all(&dir);
        }
    }

    fn paths(mail_cache_path: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let dir = mail_cache_path.join("share-ext");
        let atts = dir.join("atts");
        let draft = dir.join("draft");

        (dir, atts, draft)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IosShareExtDraft {
    pub subject: Option<String>,
    pub body: Option<String>,
    pub inline_attachments: Vec<IosShareExtAttachment>,
    pub attachments: Vec<IosShareExtAttachment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IosShareExtAttachment {
    pub path: PathBuf,
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn smoke() {
        let dir = TempDir::new().unwrap();
        let mail_cache_path = dir.path().join("mail-cache");

        fs::create_dir(&mail_cache_path).unwrap();

        // ---

        let atts_dir = IosShareExtension::init_draft(&mail_cache_path).unwrap();

        let att_oak = atts_dir.join("oak.bmp");
        let att_birch = atts_dir.join("birch.jpg");
        let att_mangrove = atts_dir.join("mangrove.tiff");

        fs::write(&att_oak, "i'm oak").unwrap();
        fs::write(&att_birch, "i'm birch").unwrap();
        fs::write(&att_mangrove, "i'm mangrove").unwrap();

        // ---

        assert!(
            IosShareExtension::load_draft(&mail_cache_path)
                .unwrap()
                .is_none()
        );

        let draft = IosShareExtDraft {
            subject: Some("Dear Connor Murphy".into()),
            body: Some("I'm sending pictures of the most amazing trees!".into()),
            inline_attachments: vec![IosShareExtAttachment {
                path: att_oak.clone(),
                name: None,
            }],
            attachments: vec![
                IosShareExtAttachment {
                    path: att_birch.clone(),
                    name: Some("birch-my-favourite.jpg".into()),
                },
                IosShareExtAttachment {
                    path: att_mangrove.clone(),
                    name: None,
                },
            ],
        };

        IosShareExtension::save_draft(&mail_cache_path, draft.clone()).unwrap();

        assert_eq!(
            Some(draft),
            IosShareExtension::load_draft(&mail_cache_path).unwrap()
        );

        assert_eq!("i'm oak", fs::read_to_string(att_oak).unwrap());
        assert_eq!("i'm birch", fs::read_to_string(att_birch).unwrap());
        assert_eq!("i'm mangrove", fs::read_to_string(att_mangrove).unwrap());

        // ---

        IosShareExtension::delete_draft(&mail_cache_path);

        assert!(
            IosShareExtension::load_draft(&mail_cache_path)
                .unwrap()
                .is_none()
        );
    }
}
