//! Cache to store values in file system
//!
//! It's built around `quick-cache` crate
//! To be stored a value should have a key implementing `CacheKey` trait.
//! As it is a cache, value can be removed at any insertion/replacement of another value.
//! The weight of the value is limited to an u32 (4GB) and the maximum total of values weight is defined at cache creation.
//!
//! A typical usage of this structure:
//!   * Create the cache
//!   * Where the data you want to store, try to get the data from cache using `get_item`
//!   * If `get_item` return `None`:
//!      + Generate/fetch the data
//!      + Store the data in cache using `add_item`
//!      + Use the data
//!   * Else, juste use the data

use quick_cache::sync::Cache;
use quick_cache::{DefaultHashBuilder, Lifecycle, OptionsBuilder, Weighter};
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs::{create_dir_all, remove_file, set_permissions, File, OpenOptions, Permissions};
use std::future::Future;
use std::hash::Hash;
use std::io::{Read, Write};
use std::marker::PhantomData;
#[cfg(target_family = "unix")]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use tracing::{error, warn};

/// Errors from `ProtonCache`
#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum CacheError {
    /// Error from IO
    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),

    /// Error from `QuickCache`
    #[error("QuickCache Error: {0}")]
    QuickCache(anyhow::Error),

    /// Error return by a callback
    #[error("Callback Error: {0}")]
    Callback(anyhow::Error),
}

#[allow(clippy::module_name_repetitions)]
pub type CacheResult<T> = Result<T, CacheError>;

/// Selection of available strategy for weighting
pub enum WeightingStrategy {
    /// Use size of the stored item
    Size,
    /// No eviction
    Zero,
}

/// Trait to configure key and extra-metadata for a cache
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::missing_errors_doc)]
pub trait CacheConfig: Clone {
    /// Type of key
    type Key: CacheKey;
    /// Type of the resource needed to get existing items.
    type Init;

    /// Get existing items, used at reload (in `ProtonCache::from_existing`).
    fn get_existing(init: Self::Init) -> impl Future<Output = CacheResult<Vec<Self::Key>>>;

    /// Handle items that should be there, but where not found, used at reload
    /// (in `ProtonCache::from_existing`).
    fn handle_failed(failed: Vec<Self::Key>) -> impl Future<Output = CacheResult<()>>;

    /// Convert the Key into a filename
    fn key_to_filename(key: &Self::Key) -> OsString;

    /// Strategy used to determine weight of an item
    #[must_use]
    fn weighting_strategy() -> WeightingStrategy {
        WeightingStrategy::Size
    }
}

#[allow(clippy::module_name_repetitions)]
pub trait CacheKey: Clone + Debug + Eq + Hash + PartialEq {
    /// Callback executed after this key is evicted.
    #[allow(clippy::unused_async)]
    fn after_evict(&self) {}
}

/// Metadata about one value, stored in memory
#[derive(Clone)]
pub struct Metadata {
    /// Path to the data on disk
    file_path: PathBuf,
    /// Size of the data
    size: u64,
}

/// Weighter for a data: the size of the data
///
/// A weighter is used by a cache to define the weight of an item.
/// The cache have a maximum total weight for is content.
/// If the sum of the weight of the items go above that threshold, some items are evicted.
/// In `ProtonCache` case, the weight is the size of the item stored in file system.
#[derive(Clone)]
pub struct DefaultWeighter<Config>
where
    Config: CacheConfig,
{
    phantom_data: PhantomData<Config>,
}

impl<Config> DefaultWeighter<Config>
where
    Config: CacheConfig,
{
    fn new() -> Self {
        Self {
            phantom_data: PhantomData,
        }
    }
}

impl<Config> Weighter<Config::Key, Metadata> for DefaultWeighter<Config>
where
    Config: CacheConfig,
{
    #[allow(clippy::cast_possible_truncation)]
    fn weight(&self, _config: &Config::Key, val: &Metadata) -> u64 {
        match Config::weighting_strategy() {
            // Weight is the size of the file
            WeightingStrategy::Size => val.size,
            // 0 is unweighted i.e. live forever
            WeightingStrategy::Zero => 0,
        }
    }
}

/// On eviction: remove file from disk
///
/// In `quick-cache`, a struct implementing `Lifecycle` trait is used to interact with events in the lifetime of an item
/// I.e. on request (before/after insert/replace) and on eviction (before/on)
/// In our case, we want to remove file from file system on eviction.
#[derive(Clone, Default)]
pub struct DefaultLifecycle<Config>
where
    Config: CacheConfig,
{
    phantom_data: PhantomData<Config>,
}

impl<Config> DefaultLifecycle<Config>
where
    Config: CacheConfig,
{
    fn new() -> Self {
        Self {
            phantom_data: PhantomData,
        }
    }
}

impl<Config> Lifecycle<Config::Key, Metadata> for DefaultLifecycle<Config>
where
    Config: CacheConfig,
{
    type RequestState = Option<PathBuf>;

    fn begin_request(&self) -> Self::RequestState {
        None
    }

    fn before_evict(
        &self,
        state: &mut Self::RequestState,
        _config: &Config::Key,
        val: &mut Metadata,
    ) {
        *state = Some(val.file_path.clone());
    }

    fn on_evict(&self, state: &mut Self::RequestState, key: Config::Key, _val: Metadata) {
        if let Some(path) = state {
            // ToDo: ET-292 On eviction, move file (in case file is still in use)
            if let Err(error) = remove_file(path) {
                error!("Couldn't remove file for key {key:?}: {error:?}");
            }
            key.after_evict();
        }
    }
}

/// A cache structure to store and retrieve data
#[allow(clippy::module_name_repetitions)]
pub struct ProtonCache<Config>
where
    Config: CacheConfig,
{
    /// `QuickCache` structure
    #[allow(clippy::type_complexity)]
    cache: Cache<
        Config::Key,
        Metadata,
        DefaultWeighter<Config>,
        DefaultHashBuilder,
        DefaultLifecycle<Config>,
    >,
    /// Path to the root of the cache
    cache_buf: PathBuf,
}

impl<Config> ProtonCache<Config>
where
    Config: CacheConfig,
{
    /// Initialize a new empty cache
    ///
    /// # Params:
    /// * `path_buf` - Path to the root of the cache
    /// * `size`     - Allocated space for cache
    ///                (Warning, don't take in account padding from FS blocks)
    ///
    /// # Errors
    /// * Can't create in memory cache
    /// * Can't create data structure on disk
    fn _new(cache_buf: PathBuf, size: u32) -> CacheResult<Self> {
        // create in memory cache
        let cache = Cache::with_options(
            OptionsBuilder::new()
                .estimated_items_capacity(size as usize)
                .weight_capacity(u64::from(size))
                .build()
                .map_err(|e| CacheError::QuickCache(e.into()))?,
            DefaultWeighter::new(),
            DefaultHashBuilder::default(),
            DefaultLifecycle::new(),
        );

        // create file directory
        create_dir_all(cache_buf.clone())?;
        // ToDo: ET-296 Do windows counterpart
        if cfg!(unix) {
            set_permissions(cache_buf.clone(), Permissions::from_mode(0o700))?;
        }

        Ok(Self { cache, cache_buf })
    }

    /// Initialize a new cache from existing keys.
    ///
    /// Return the new cache and the list of the files that don't exist.
    ///
    /// # Params:
    /// * `path_buf` - Path to the root of the cache.
    /// * `size`     - Allocated space for cache.
    ///                (Warning, don't take in account padding from FS blocks)
    /// * `existing` - List of item expected to be present.
    ///
    /// # Errors
    /// * Can't create in memory cache
    /// * Can't create data structure on disk
    ///
    pub async fn new(cache_buf: PathBuf, size: u32, init: Config::Init) -> CacheResult<Self> {
        let existing = Config::get_existing(init).await?;
        let cache = Self::_new(cache_buf, size)?;

        let mut failed = vec![];
        for key in existing {
            if !cache.add_existing_item(key.clone()) {
                failed.push(key.clone());
            }
        }
        Config::handle_failed(failed).await?;
        Ok(cache)
    }

    /// Add an item to the cache
    ///
    /// # Params:
    /// * `key`: unique identifier for the item
    /// * `value`: the item
    ///
    /// # Errors
    /// * Can't create file on disk
    /// * Can't write value in file
    pub fn add_item(&self, key: Config::Key, value: &[u8]) -> CacheResult<PathBuf> {
        let file_path = self.path_from_key(&key);
        // ToDo: ET-296 Do windows counterpart
        let mut file = if cfg!(unix) {
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(file_path.clone())?
        } else {
            File::create(file_path.clone())?
        };
        file.write_all(value)?;
        let metadata = Metadata {
            file_path: file_path.clone(),
            size: value.len() as u64,
        };
        self.cache.insert(key, metadata);
        Ok(file_path)
    }

    /// Add metadata in cache for an item already existing
    ///
    /// # Params:
    /// * `key`: unique identifier for the item
    ///
    pub fn add_existing_item(&self, key: Config::Key) -> bool {
        let file_path = self.path_from_key(&key);

        let Ok(metadata) = file_path.metadata() else {
            warn!("Cache item {key:?} don't exist");
            return false;
        };

        let metadata = Metadata {
            file_path: file_path.clone(),
            size: metadata.len(),
        };
        self.cache.insert(key, metadata);
        true
    }

    /// Retrieve the value associated with key from cache
    ///
    /// # params:
    /// * `key`: unique identifier for the item
    ///
    /// # Errors
    /// * Can't open file containing value
    pub fn get_item(&self, key: &Config::Key) -> CacheResult<Option<impl Read>> {
        self.cache
            .get(key)
            .map(|m| File::open(m.file_path).map_err(CacheError::IO))
            .transpose()
    }

    /// Retrieve a path toward the file containing the value
    /// Can be used to pass to a Native component
    ///
    /// # params:
    /// * `key`: unique identifier for the item
    #[must_use]
    pub fn get_item_path(&self, key: &Config::Key) -> Option<PathBuf> {
        self.cache.get(key).map(|v| v.file_path)
    }

    /// Remove a value from cache
    ///
    /// # params:
    /// * `key`: key of the removed item
    ///
    /// # Errors
    /// * Can't remove file from file system
    pub fn remove(&self, key: &Config::Key) -> CacheResult<()> {
        // Eviction is not called in this case
        if let Some(path) = self.get_item_path(key) {
            // ToDo: ET-292 On eviction, move file (in case file is still in use)
            remove_file(path)?;
            key.after_evict();
        }
        self.cache.remove(key);
        Ok(())
    }

    /// Return the count of stored values
    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check is cache is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get the path corresponding to a key
    pub fn path_from_key(&self, key: &Config::Key) -> PathBuf {
        let filename = Config::key_to_filename(key);
        self.cache_buf.clone().join(filename)
    }
}
