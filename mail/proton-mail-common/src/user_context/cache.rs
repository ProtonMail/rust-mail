use crate::MailContextResult;
use proton_api_mail::services::proton::requests::GetImagesLogoOptions;
use proton_core_common::cache::{CacheKey, ProtonCache, WeightingStrategy};
use std::ffi::OsString;
use std::path::PathBuf;
use uuid::Uuid;

const ATTACHMENTS_CACHE_RATIO: u32 = 1;
const USER_IMAGE_CACHE_RATIO: u32 = 1;
const CACHE_RATIO_SUM: u32 = ATTACHMENTS_CACHE_RATIO + USER_IMAGE_CACHE_RATIO;

/// Structure to group all caches
pub struct Cache {
    /// Cache for message bodies
    pub messages_cache: ProtonCache<MessageKey>,
    /// Cache for user images
    pub images_logo_cache: ProtonCache<ImagesLogoKey>,
    /// cache for attachments
    pub attachments_cache: ProtonCache<AttachmentKey>,
}

impl Cache {
    pub fn new(root_path: PathBuf, size: u32) -> MailContextResult<Self> {
        // Since message body are weightless, any size would do the same, i.e. live forever
        let messages_cache = ProtonCache::new(root_path.join("messages"), size)?;

        let user_images_cache = ProtonCache::new(
            root_path.join("user_images"),
            size * USER_IMAGE_CACHE_RATIO / CACHE_RATIO_SUM,
        )?;

        let attachments_cache = ProtonCache::new(
            root_path.join("attachments"),
            size * ATTACHMENTS_CACHE_RATIO / CACHE_RATIO_SUM,
        )?;

        Ok(Self {
            messages_cache,
            images_logo_cache: user_images_cache,
            attachments_cache,
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AttachmentKey(pub u64);
impl CacheKey for AttachmentKey {
    fn to_filename(&self) -> OsString {
        format!("{}", self.0).into()
    }
}

/// Cache key for User Images
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImagesLogoKey(pub GetImagesLogoOptions);
impl CacheKey for ImagesLogoKey {
    fn to_filename(&self) -> OsString {
        // `AddressDomainLogoDetails` contains to many possible configuration to build a unique filename from it
        Uuid::new_v4().to_string().into()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MessageKey(pub u64);
impl CacheKey for MessageKey {
    fn to_filename(&self) -> OsString {
        format!("{}", self.0).into()
    }

    fn weighting_strategy() -> WeightingStrategy {
        WeightingStrategy::Zero
    }
}
