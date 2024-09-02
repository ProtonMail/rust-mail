use crate::MailContextResult;
use proton_core_common::cache::{CacheConfig, ProtonCache, WeightingStrategy};
use proton_core_common::datatypes::LocalId;
use std::ffi::OsString;
use std::path::PathBuf;

/// Structure to group all caches
pub struct Cache {
    /// Cache for message bodies
    pub messages_cache: ProtonCache<CacheMessageConfig>,
    /// cache for attachments
    pub attachments_cache: ProtonCache<CacheAttachmentConfig>,
}

impl Cache {
    pub fn new(root_path: PathBuf, size: u32) -> MailContextResult<Self> {
        // Since message body are weightless, any size would do the same, i.e. live forever
        let messages_cache = ProtonCache::new(root_path.join("messages"), size)?;

        let attachments_cache = ProtonCache::new(root_path.join("attachments"), size)?;

        Ok(Self {
            messages_cache,
            attachments_cache,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheAttachmentConfig;
impl CacheConfig for CacheAttachmentConfig {
    type Key = CacheAttachmentKey;

    // TODO: Cloning VerificationResult provoke a loop between Clone and ToOwned
    // type ExtraMetadata = Arc<Mutex<Option<VerificationResult>>>;
    type ExtraMetadata = ();

    fn key_to_filename(key: &Self::Key) -> OsString {
        format!("{}-{}", key.attachment_id, key.filename).into()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheAttachmentKey {
    attachment_id: u64,
    filename: String,
}

impl CacheAttachmentKey {
    pub fn new(attachment_id: LocalId, filename: &str) -> Self {
        Self {
            attachment_id: attachment_id.as_u64(),
            filename: filename.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheMessageConfig;
impl CacheConfig for CacheMessageConfig {
    type Key = u64;
    type ExtraMetadata = ();

    fn key_to_filename(key: &Self::Key) -> OsString {
        format!("{key}").into()
    }

    fn weighting_strategy() -> WeightingStrategy {
        WeightingStrategy::Zero
    }
}
