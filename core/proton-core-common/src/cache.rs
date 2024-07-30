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
use std::fs::{
    create_dir_all, remove_dir_all, remove_file, set_permissions, File, OpenOptions, Permissions,
};
use std::hash::Hash;
use std::io::{Read, Write};
use std::marker::PhantomData;
#[cfg(target_family = "unix")]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use tracing::error;

/// Name of the file containing data needed by cache to load from disk
const CACHE_METADATA_FILE: &str = ".proton.cache.metadata";

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
}

type Result<T> = std::result::Result<T, CacheError>;

/// Selection of available strategy for weighting
pub enum WeightingStrategy {
    /// Use size of the stored item
    Size,
    /// No eviction
    Zero,
}

/// Trait to configure key and extra-metadata for a cache
#[allow(clippy::module_name_repetitions)]
pub trait CacheConfig: Clone {
    type Key: Clone + Debug + Eq + Hash + PartialEq;
    type ExtraMetadata: Clone + Debug + Default;

    /// Convert the Key into a filename
    fn key_to_filename(key: &Self::Key) -> OsString;

    /// Strategy used to determine weight of an item
    #[must_use]
    fn weighting_strategy() -> WeightingStrategy {
        WeightingStrategy::Size
    }
}

/// Metadata about one value, stored in memory
#[derive(Clone)]
pub struct Metadata<T> {
    /// Path to the data on disk
    file_path: PathBuf,
    /// Size of the data
    size: u64,
    /// Additional data
    additional: T,
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

impl<Config> Weighter<Config::Key, Metadata<Config::ExtraMetadata>> for DefaultWeighter<Config>
where
    Config: CacheConfig,
{
    #[allow(clippy::cast_possible_truncation)]
    fn weight(&self, _config: &Config::Key, val: &Metadata<Config::ExtraMetadata>) -> u32 {
        match Config::weighting_strategy() {
            // Value more than u32::MAX bytes will be counted as u32::MAX (4GB)
            WeightingStrategy::Size => val.size.clamp(1, u64::from(u32::MAX)) as u32,
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

impl<Config> Lifecycle<Config::Key, Metadata<Config::ExtraMetadata>> for DefaultLifecycle<Config>
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
        val: &mut Metadata<Config::ExtraMetadata>,
    ) {
        *state = Some(val.file_path.clone());
    }

    fn on_evict(
        &self,
        state: &mut Self::RequestState,
        key: Config::Key,
        _val: Metadata<Config::ExtraMetadata>,
    ) {
        if let Some(path) = state {
            // ToDo: ET-292 On eviction, move file (in case file is still in use)
            if let Err(error) = remove_file(path) {
                error!("Couldn't remove file for key {key:?}: {error:?}");
            }
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
        Metadata<Config::ExtraMetadata>,
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
    /// Initialize a new cache
    ///
    /// # Params:
    /// * `path_buf`: Path to the root of the cache
    /// * `size`: Allocated space for cache (Warning, don't take in account padding from FS blocks)
    ///
    /// # Errors
    /// * Can't create in memory cache
    /// * Can't create data structure on disk
    pub fn new(cache_buf: PathBuf, size: u32) -> Result<Self> {
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
        let mut file = cache_buf.clone();
        file.push(CACHE_METADATA_FILE);
        let mut file = File::create(file)?;
        write!(file, "{size}")?;

        Ok(Self { cache, cache_buf })
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
    pub fn add_item(&self, key: Config::Key, value: &[u8]) -> Result<PathBuf> {
        self.add_item_with_extra_metadata(key, value, Default::default())
    }

    /// Add an item to the cache with some additional information to keep in memory
    ///
    /// # Params:
    /// * `key`: unique identifier for the item
    /// * `value`: the item
    /// * `additional`: extra information to store
    ///
    /// # Errors
    /// * Can't create file on disk
    /// * Can't write value in file
    pub fn add_item_with_extra_metadata(
        &self,
        key: Config::Key,
        value: &[u8],
        additional: Config::ExtraMetadata,
    ) -> Result<PathBuf> {
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
            additional,
        };
        self.cache.insert(key, metadata);
        Ok(file_path)
    }

    /// Retrieve the value associated with key from cache
    ///
    /// # params:
    /// * `key`: unique identifier for the item
    ///
    /// # Errors
    /// * Can't open file containing value
    pub fn get_item(&self, key: &Config::Key) -> Result<Option<impl Read>> {
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

    /// Retrieve additional metadata stored
    ///
    /// # params:
    ///   *  `key`: unique identifier for the item
    pub fn get_item_metadata(&self, key: &Config::Key) -> Option<Config::ExtraMetadata> {
        self.cache.get(key).map(|v| v.additional)
    }

    /// Remove a value from cache
    ///
    /// # params:
    /// * `key`: key of the removed item
    ///
    /// # Errors
    /// * Can't remove file from file system
    pub fn remove(&self, key: &Config::Key) -> Result<()> {
        // Eviction is not called in this case
        if let Some(path) = self.get_item_path(key) {
            // ToDo: ET-292 On eviction, move file (in case file is still in use)
            remove_file(path)?;
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

// ToDo(Et-298): Cache Reload
// As long as we have no way to reload cache ... purging it at exit
impl<Config> Drop for ProtonCache<Config>
where
    Config: CacheConfig,
{
    fn drop(&mut self) {
        if let Err(error) = remove_dir_all(self.cache_buf.clone()) {
            error!("Couldn't remove cache folder: {error}");
        }
    }
}
