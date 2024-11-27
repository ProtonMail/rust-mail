use crate::models::{Attachment, Message};
use crate::{AppError, MailContextResult};
use anyhow::anyhow;
use futures::executor::block_on;
use proton_core_common::cache::{
    CacheConfig, CacheError, CacheKey, CacheResult, ProtonCache, WeightingStrategy,
};
use proton_core_common::datatypes::LocalId;
use stash::orm::Model;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};
use std::ffi::OsString;
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
    /// * `interfaces` - interface used to access database.
    pub async fn new<A>(root_path: PathBuf, size: u32, interface: &A) -> MailContextResult<Self>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        // Since message body are weightless, any size would do the same, i.e. live forever
        let messages_cache =
            ProtonCache::new(root_path.join("messages"), size, interface.stash().clone()).await?;

        let attachments_cache = ProtonCache::new(
            root_path.join("attachments"),
            size,
            interface.stash().clone(),
        )
        .await?;

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
    type Interface = Stash;
    type ExtraMetadata = ();

    async fn get_existing(stash: Stash) -> CacheResult<Vec<Self::Key>> {
        CacheAttachmentKey::get_all_cached(&stash)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
    }

    async fn handle_failed(failed: Vec<Self::Key>, _: Stash) -> CacheResult<()> {
        CacheAttachmentKey::batch_unset(failed)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
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
    stash: Stash,
}

impl CacheAttachmentKey {
    /// Create a new `CacheAttachmentKey`
    ///
    /// # params
    /// * `attachment_id` - local id of the corresponding Attachment.
    /// * `filename`      - original filename for the corresponding Attachment.
    /// * `stash`         - stash where the corresponding Attachment is recorded.
    ///
    pub fn new(attachment_id: LocalId, filename: &str, stash: Stash) -> Self {
        Self {
            attachment_id: attachment_id.as_u64(),
            filename: filename.to_owned(),
            stash,
        }
    }

    /// Get all Attachments that are currently cached.
    async fn get_all_cached<A>(interface: &A) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let cached = Attachment::find("WHERE cached = true", vec![], interface, None).await?;
        let stash = interface.stash();
        Ok(cached
            .iter()
            .map(|v| Self::from_attachment(v, stash))
            .collect())
    }

    /// Unset cached state for a batch of Attachments.
    async fn batch_unset(keys: impl IntoIterator<Item = Self>) -> Result<(), AppError> {
        for key in keys {
            key.unset_cached().await?;
        }
        Ok(())
    }

    /// Create a `CacheAttachmentKey` for an `Attachment`
    ///
    /// # params
    /// * `attachment` - The `Attachment`.
    /// * `stash`      - Stash where the `Attachment` is recorded.
    ///
    pub(crate) fn from_attachment(attachment: &Attachment, stash: &Stash) -> Self {
        Self {
            attachment_id: attachment.local_id.expect("Should be set").as_u64(),
            filename: attachment.filename.clone(),
            stash: stash.clone(),
        }
    }

    /// Set self as cached.
    pub(crate) async fn set_cached(&self) -> Result<(), AppError> {
        self.set_cache_status(true).await
    }

    /// Set self as not cached.
    pub(crate) async fn unset_cached(&self) -> Result<(), AppError> {
        self.set_cache_status(false).await
    }

    /// Set the cached status of this `Attachment`.
    async fn set_cache_status(&self, status: bool) -> Result<(), AppError> {
        let transaction = self.stash.transaction().await?;
        let attachment = Attachment::load(self.attachment_id.into(), &transaction)
            .await
            .inspect_err(|e| error!("Couldn't load Attachment: {e}"))?;
        let Some(mut attachment) = attachment else {
            error!("Couldn't load attachment {}", self.attachment_id);
            return Err(AppError::AttachmentMissing(self.attachment_id.into()));
        };
        attachment.cached = status;
        attachment
            .save_using(&transaction)
            .await
            .inspect_err(|e| error!("Couldn't save attachment: {e}"))?;
        transaction.commit().await?;
        Ok(())
    }
}
impl Hash for CacheAttachmentKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.attachment_id.hash(state);
        self.filename.hash(state);
    }
}

impl CacheKey for CacheAttachmentKey {
    fn after_evict(&self, _: &Stash) {
        block_on(async {
            let _ = self.unset_cached().await;
        })
    }
}

/// Configuration for the cache storing MessageBody.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CacheMessageConfig;
impl CacheConfig for CacheMessageConfig {
    type Key = CacheMessageKey;
    type Interface = Stash;
    type ExtraMetadata = ();

    async fn get_existing(stash: Stash) -> CacheResult<Vec<Self::Key>> {
        CacheMessageKey::get_all_cached(&stash)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
    }

    async fn handle_failed(failed: Vec<Self::Key>, _: Stash) -> CacheResult<()> {
        CacheMessageKey::batch_unset(failed)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
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
    stash: Stash,
}

impl CacheMessageKey {
    /// Get all currently cached MessageBody.
    async fn get_all_cached<A>(interface: &A) -> Result<Vec<Self>, StashError>
    where
        A: Interface + Into<AgnosticInterface>,
    {
        let cached = Message::find("WHERE cached = true", vec![], interface, None).await?;
        let stash = interface.stash();
        Ok(cached
            .iter()
            .map(|v| Self::from_message(v, stash))
            .collect())
    }

    /// Generate a key for a Message
    pub fn from_message<A>(message: &Message, interface: &A) -> Self
    where
        A: Interface + Into<AgnosticInterface>,
    {
        Self {
            message_id: message.local_id.expect("Should be set").as_u64(),
            stash: interface.stash().clone(),
        }
    }

    /// Set a batch of MessageBody as uncached.
    async fn batch_unset(keys: impl IntoIterator<Item = Self>) -> Result<(), AppError> {
        for key in keys {
            key.unset_cached().await?
        }
        Ok(())
    }

    /// Set a MessageBody as uncached.
    async fn unset_cached(&self) -> Result<(), AppError> {
        self.set_cached_status(false).await
    }

    /// Set a MessageBody as cached.
    pub(crate) async fn set_cached(&self) -> Result<(), AppError> {
        self.set_cached_status(true).await
    }

    /// Set the cached status of a MessageBody.
    async fn set_cached_status(&self, status: bool) -> Result<(), AppError> {
        let transaction = self.stash.transaction().await?;
        let message = Message::load(self.message_id.into(), &transaction)
            .await
            .inspect_err(|e| error!("Couldn't load message: {e}"))?;
        let Some(mut message) = message else {
            error!("Couldn't load message {}", self.message_id);
            return Err(AppError::MessageMissing(self.message_id.into()));
        };
        message.cached = status;
        message
            .save_using(&transaction)
            .await
            .inspect_err(|e| error!("Couldn't save message: {e}"))?;
        transaction.commit().await?;
        Ok(())
    }
}

impl Hash for CacheMessageKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.message_id.hash(state);
    }
}

impl CacheKey for CacheMessageKey {
    fn after_evict(&self, _: &Stash) {
        block_on(async {
            let _ = self.unset_cached().await;
        })
    }
}
