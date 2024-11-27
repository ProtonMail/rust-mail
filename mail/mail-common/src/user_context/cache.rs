use crate::models::{Attachment, Message};
use crate::{AppError, MailContextResult};
use anyhow::anyhow;
use proton_core_common::cache::{
    CacheConfig, CacheError, CacheKey, CacheResult, ProtonCache, WeightingStrategy,
};
use proton_core_common::datatypes::LocalId;
use std::ffi::OsString;
use std::fs::{read_dir, remove_file, DirEntry};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use tracing::error;

/// Structure to group all caches
pub struct Cache {
    /// Cache for message bodies
    pub messages_cache: ProtonCache<CacheMessageConfig>,
    /// cache for attachments
    pub attachments_cache: ProtonCache<CacheAttachmentConfig>,
}

impl Cache {
    /// Create a new Cache for `MessageBody` and `Attachment`
    ///
    /// # params
    /// * `root_path`  - path to the folder that will contain the caches.
    /// * `size`       - maximum size for the caches.
    ///
    pub async fn new(root_path: PathBuf, size: u32) -> MailContextResult<Self> {
        let messages_path = root_path.join("messages");
        // Since message body are weightless, any size would do the same, i.e. live forever
        let messages_cache = ProtonCache::new(messages_path.clone(), size, messages_path).await?;

        let attachments_path = root_path.join("attachments");
        let attachments_cache =
            ProtonCache::new(attachments_path.clone(), size, attachments_path).await?;

        Ok(Self {
            messages_cache,
            attachments_cache,
        })
    }
}

/// Configuration for the cache storing Attachments.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheAttachmentConfig;
impl CacheConfig for CacheAttachmentConfig {
    type Key = CacheAttachmentKey;
    type Init = PathBuf;
    type ExtraMetadata = ();

    async fn get_existing(root_path: PathBuf) -> CacheResult<Vec<Self::Key>> {
        CacheAttachmentKey::get_all_cached(root_path.clone())
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
    }

    async fn handle_failed(failed: Vec<Self::Key>) -> CacheResult<()> {
        error!("Couldn't load existing files({failed:?}), removing them");
        for key in failed {
            let file = Self::key_to_filename(&key, None)?;
            drop(remove_file(file).inspect_err(|e| error!("Couldn't remove file: {e}")));
        }
        Ok(())
    }

    fn key_to_filename(key: &Self::Key, _extra: Option<&()>) -> CacheResult<OsString> {
        Ok(format!("{}-{}", key.attachment_id, key.filename).into())
    }
}

/// A key for the `CacheAttachmentConfig`
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheAttachmentKey {
    attachment_id: u64,
    filename: String,
}

impl CacheAttachmentKey {
    /// Create a new `CacheAttachmentKey`
    ///
    /// # params
    /// * `attachment_id` - local id of the corresponding Attachment.
    /// * `filename`      - original filename for the corresponding Attachment.
    ///
    pub fn new(attachment_id: LocalId, filename: &str) -> Self {
        Self {
            attachment_id: attachment_id.as_u64(),
            filename: filename.to_owned(),
        }
    }

    /// Get all Attachments that are currently cached.
    async fn get_all_cached(root_path: PathBuf) -> Result<Vec<Self>, AppError> {
        let mut keys = vec![];
        for entry in read_dir(root_path.clone())
            .inspect_err(|e| error!("Could not read dir({root_path:?}): {e}"))?
        {
            let entry = entry?;
            let Some(key) = Self::from_dir_entry(entry) else {
                continue;
            };
            keys.push(key);
        }
        Ok(keys)
    }

    fn from_dir_entry(entry: DirEntry) -> Option<Self> {
        let filetype = entry
            .file_type()
            .inspect_err(|e| error!("Can't get file type for dir entry({entry:?}): {e}"))
            .ok()?;
        if filetype.is_file() {
            let filename = entry.file_name();
            if let Some(filename) = filename.to_str() {
                Self::from_filename(filename)
            } else {
                error!("Can't parse os filename({filename:?}) as an attachment cache key");
                None
            }
        } else {
            None
        }
    }

    fn from_filename(filename: &str) -> Option<Self> {
        let Some((id, filename)) = filename.split_once('-') else {
            error!("Can't parse filename({filename:?}) as a attachment cache key");
            return None;
        };
        let attachment_id = id
            .parse()
            .inspect_err(|_| error!("Can't parse str({id:?}) as an attachment id"))
            .ok()?;
        Some(Self {
            attachment_id,
            filename: filename.to_owned(),
        })
    }
}

impl From<&Attachment> for CacheAttachmentKey {
    fn from(attachment: &Attachment) -> Self {
        Self {
            attachment_id: attachment.local_id.expect("Should be set").as_u64(),
            filename: attachment.filename.clone(),
        }
    }
}

impl Hash for CacheAttachmentKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.attachment_id.hash(state);
        self.filename.hash(state);
    }
}

impl CacheKey for CacheAttachmentKey {}

/// Configuration for the cache storing MessageBody.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheMessageConfig;
impl CacheConfig for CacheMessageConfig {
    type Key = CacheMessageKey;
    type Init = PathBuf;
    type ExtraMetadata = ();

    async fn get_existing(root_path: PathBuf) -> CacheResult<Vec<Self::Key>> {
        CacheMessageKey::get_all_cached(root_path.clone())
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
    }

    async fn handle_failed(failed: Vec<Self::Key>) -> CacheResult<()> {
        error!("Couldn't load existing files({failed:?}), removing them");
        for key in failed {
            let file = Self::key_to_filename(&key, None)?;
            drop(remove_file(file).inspect_err(|e| error!("Couldn't remove file: {e}")));
        }
        Ok(())
    }

    fn key_to_filename(key: &Self::Key, _extra: Option<&()>) -> CacheResult<OsString> {
        Ok(format!("{}", key.message_id).into())
    }

    fn weighting_strategy() -> WeightingStrategy {
        WeightingStrategy::Zero
    }
}

/// Key for a MessageBody in CacheMessageConfig.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheMessageKey {
    message_id: u64,
}

impl CacheMessageKey {
    /// Get all currently cached MessageBody.
    async fn get_all_cached(root_path: PathBuf) -> Result<Vec<Self>, AppError> {
        let mut keys = vec![];
        for entry in read_dir(root_path.clone())
            .inspect_err(|e| error!("Could not read dir({root_path:?}) : {e}"))?
        {
            let entry = entry?;
            let Some(key) = Self::from_dir_entry(entry) else {
                continue;
            };
            keys.push(key);
        }
        Ok(keys)
    }

    fn from_dir_entry(entry: DirEntry) -> Option<Self> {
        let filetype = entry
            .file_type()
            .inspect_err(|e| error!("Can't get file type for dir entry({entry:?}): {e}"))
            .ok()?;
        if filetype.is_file() {
            let filename = entry.file_name();
            if let Some(filename) = filename.to_str() {
                let message_id = filename
                    .parse()
                    .inspect_err(|_| {
                        error!("Can't parse filename ({filename:?}) as a message cache key")
                    })
                    .ok()?;
                Some(Self { message_id })
            } else {
                error!("Can't parse os filename ({filename:?}) as a message cache key");
                None
            }
        } else {
            None
        }
    }
}

impl From<&Message> for CacheMessageKey {
    fn from(message: &Message) -> Self {
        Self {
            message_id: message.local_id.expect("Should be set").as_u64(),
        }
    }
}

impl Hash for CacheMessageKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.message_id.hash(state);
    }
}

impl CacheKey for CacheMessageKey {}
