use futures::future::join_all;
use proton_core_common::cache::{
    CacheConfig, CacheKey, CacheResult, ProtonCache, WeightingStrategy,
};
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs::{File, read_dir};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::sync::Arc;
use std::thread::spawn;
use tempdir::TempDir;

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct TestKey(OsString);
impl CacheKey for TestKey {}

impl From<&str> for TestKey {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl From<String> for TestKey {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct TestConfig;

impl CacheConfig for TestConfig {
    type Key = TestKey;

    type Resource = Vec<TestKey>;
    type ExtraMetadata = ();

    async fn get_existing(resource: Vec<TestKey>) -> CacheResult<Vec<TestKey>> {
        Ok(resource)
    }

    async fn handle_failed(_failed: Vec<TestKey>, _resource: Self::Resource) -> CacheResult<()> {
        Ok(())
    }

    fn key_to_filename(
        key: &Self::Key,
        _extra: Option<&Self::ExtraMetadata>,
    ) -> CacheResult<OsString> {
        Ok(key.0.clone())
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct TestWeightlessKey;

impl CacheConfig for TestWeightlessKey {
    type Key = TestKey;
    type Resource = Vec<TestKey>;
    type ExtraMetadata = ();
    async fn get_existing(resource: Vec<TestKey>) -> CacheResult<Vec<TestKey>> {
        Ok(resource)
    }

    async fn handle_failed(_failed: Vec<TestKey>, _resource: Self::Resource) -> CacheResult<()> {
        Ok(())
    }

    fn key_to_filename(
        key: &Self::Key,
        _extra: Option<&Self::ExtraMetadata>,
    ) -> CacheResult<OsString> {
        Ok(key.0.clone())
    }

    fn weighting_strategy() -> WeightingStrategy {
        WeightingStrategy::Zero
    }
}

#[derive(Eq, Debug, Clone)]
struct TestExtraMetadata {
    a: u8,
    b: u8,
}

impl Hash for TestExtraMetadata {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.a.hash(state);
    }
}

impl PartialEq for TestExtraMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.a == other.a
    }
}

impl CacheKey for TestExtraMetadata {}
impl CacheConfig for TestExtraMetadata {
    type Key = TestExtraMetadata;
    type Resource = Vec<TestExtraMetadata>;
    type ExtraMetadata = u8;
    async fn get_existing(resource: Self::Resource) -> CacheResult<Vec<Self::Key>> {
        Ok(resource)
    }

    async fn handle_failed(_failed: Vec<Self::Key>, _resource: Self::Resource) -> CacheResult<()> {
        Ok(())
    }

    fn key_to_filename(
        key: &Self::Key,
        extra: Option<&Self::ExtraMetadata>,
    ) -> CacheResult<OsString> {
        let extra = extra.unwrap();
        Ok(format!("{}.{extra}", key.a).into())
    }

    fn extra_for_key(key: &Self::Key) -> Option<Self::ExtraMetadata> {
        Some(key.b)
    }
}

fn get_content(mut file: impl Read) -> Vec<u8> {
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    content
}

#[tokio::test]
async fn create_cache() {
    // Setup:
    //   * Create a temporary directory for cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();

    // Action 1:
    //   * Create the cache
    let _cache = ProtonCache::<TestConfig>::new(dir.clone(), 1000, vec![])
        .await
        .unwrap();

    // Validate:
    //   * Directory exist and is empty (+1 for cache data)
    let dir = read_dir(dir).unwrap();
    assert_eq!(dir.count(), 0);
}

#[tokio::test]
async fn reload_cache() {
    // Setup:
    //   * Create a temporary directory for cache
    //   * Create some files to load
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let existing = vec![
        TestKey("key1".into()),
        TestKey("key2".into()),
        TestKey("key3".into()),
    ];
    File::create_new(dir.join("key1")).unwrap();
    File::create_new(dir.join("key3")).unwrap();

    // Action 1:
    //   * Reload the cache
    let cache = ProtonCache::<TestConfig>::new(dir.clone(), 1000, existing)
        .await
        .unwrap();

    // Validate:
    //   * Cache contains the items
    //   * File that don't exist are in failed
    assert_eq!(cache.len(), 2);
}

#[tokio::test]
async fn add_get_cache_item() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestConfig>::new(dir.clone(), 1000, vec![])
        .await
        .unwrap();
    let value = "A very big file".as_bytes();
    let key = TestKey("Key".into());

    // Actions:
    //   * Add an item into cache
    let path = cache.add_item(key.clone(), value).unwrap();

    // Validation:
    //   * Item content is given value
    //   * An existing item have a path
    //   * An non-existing item have no path
    //   * There is a file on disk (+1 for cache data)
    let got = cache.get_item(&key).unwrap().unwrap();
    let path1 = cache.get_item_path(&key);
    let path2 = cache.get_item_path(&"Foo".into());
    assert_eq!(value.to_vec(), get_content(got));
    assert_eq!(path1, Some(path));
    assert!(path2.is_none());
    let dir = read_dir(dir).unwrap();
    assert_eq!(dir.count(), 1);
    let file = cache.path_from_key(&key, None).unwrap();
    let mut file = File::open(file).unwrap();
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    assert_eq!(content, value);
}

#[tokio::test]
async fn add_item_twice() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestConfig>::new(dir.clone(), 1000, vec![])
        .await
        .unwrap();
    let value = "A very big file".as_bytes();
    let other_value = "Another very big file".as_bytes();
    let key = TestKey("Key".into());

    // Actions:
    //   * Add two different items with same key
    cache.add_item(key.clone(), value).unwrap();
    cache.add_item(key.clone(), other_value).unwrap();

    // Validation:
    //   * Only one file on disk
    let dir = read_dir(dir).unwrap();
    assert_eq!(dir.count(), 1);
    let file = cache.path_from_key(&key, None).unwrap();
    let mut file = File::open(file).unwrap();
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    assert_eq!(content, other_value);
}

#[tokio::test]
async fn eviction() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestConfig>::new(dir.clone(), 100, vec![])
        .await
        .unwrap();
    let value = "A very big file".as_bytes(); // 15 bytes
    let to_create = 100;

    // Actions:
    //   * Add many items
    for i in 0..to_create {
        cache.add_item(format!("{i}").into(), value).unwrap();
    }

    // Validation:
    //   * Only a few items are still in cache
    let dir = read_dir(dir).unwrap();
    let file_count = dir.count();
    let cache_count = cache.len();
    assert_eq!(file_count, cache_count);
    assert_eq!(cache_count, 6); // (6+1) * 15 = 105 => maximum 6 values of 15 bytes
}

#[tokio::test]
async fn weightless() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestWeightlessKey>::new(dir.clone(), 100, vec![])
        .await
        .unwrap();
    let value = "A very big file".as_bytes(); // 15 bytes
    let to_create = 100;

    // Actions:
    //   * Add many items
    for i in 0..to_create {
        cache.add_item(format!("{i}").into(), value).unwrap();
    }

    // Validation:
    //   * Only a few items are still in cache
    let dir = read_dir(dir).unwrap();
    let file_count = dir.count();
    let cache_count = cache.len();
    assert_eq!(file_count, cache_count);
    assert_eq!(cache_count, 100);
}

#[tokio::test]
async fn remove() {
    // Setup:
    //   * Create a cache with a value
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestConfig>::new(dir.clone(), 1000, vec![])
        .await
        .unwrap();
    let value = "A very big file".as_bytes(); // 15 bytes
    cache.add_item("key1".into(), value).unwrap();
    cache.add_item("key2".into(), value).unwrap();
    cache.add_item("key3".into(), value).unwrap();

    // Action:
    //   * Remove value from cache
    cache.remove(&"key2".into()).unwrap();

    // Validation:
    //   * The value is no more here
    let dir = read_dir(dir).unwrap();
    let file_count = dir.count();
    assert_eq!(cache.len(), 2);
    assert_eq!(file_count, 2);
}

#[tokio::test]
async fn concurrent_insert_same() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestConfig>::new(dir.clone(), 1000, vec![])
        .await
        .unwrap();
    let value = "A very big file".as_bytes();
    let key = TestKey("Key".into());

    // Actions:
    //   * Add same items in a concurrent way
    let mut tasks = vec![];
    for _ in 0..10 {
        let task = cache.get_path_or_insert(&key, async { Ok(value.to_vec()) });
        tasks.push(task);
    }

    // Validation:
    //   * No error happened
    //   * Content value is as expected
    for result in join_all(tasks).await {
        let file = result.unwrap();
        let mut file = File::open(file).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        assert_eq!(content, value);
    }
}

#[tokio::test]
async fn concurrent_insert_different() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = Arc::new(
        ProtonCache::<TestConfig>::new(dir.clone(), 1000, vec![])
            .await
            .unwrap(),
    );

    // Actions:
    //   * Add same items in a concurrent way
    let mut threads = vec![];
    for i in 0..10 {
        let cache = cache.clone();
        let value = format!("{i}").as_bytes().to_owned();
        let key = TestKey(format!("{i}").into());
        let thread = spawn(move || cache.add_item(key.clone(), &value));
        threads.push(thread);
    }

    // Validation:
    //   * No error happened
    //   * Content value is as expected
    for path in threads.into_iter().map(|t| t.join().unwrap()) {
        let path = path.unwrap();
        let value = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .as_bytes()
            .to_owned();
        let mut file = File::open(path).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        assert_eq!(content, value);
    }
}

#[tokio::test]
async fn use_extra_metadata() {
    #[allow(clippy::unused_async)]
    async fn with(value: Vec<u8>, extra: u8) -> CacheResult<(Vec<u8>, u8)> {
        Ok((value, extra))
    }

    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let key1 = TestExtraMetadata { a: 1, b: 2 };
    let key2 = TestExtraMetadata { a: 3, b: 4 };
    let key3 = TestExtraMetadata { a: 5, b: 6 };

    File::create_new(dir.join("1.2")).unwrap();
    File::create_new(dir.join("5.6")).unwrap();

    let cache = Arc::new(
        ProtonCache::<TestExtraMetadata>::new(dir.clone(), 1000, vec![key1.clone(), key2, key3])
            .await
            .unwrap(),
    );
    // All keys with files are now in cache
    assert_eq!(cache.len(), 2);

    let path = cache
        .get_path_or_insert_with_extra(&key1, with(vec![], 7))
        .await
        .unwrap();
    // Key is in cache -> use extra from key
    assert!(path.ends_with("1.2"));

    let key3 = TestExtraMetadata { a: 8, b: 9 };
    let path = cache
        .get_path_or_insert_with_extra(&key3, with(vec![], 10))
        .await
        .unwrap();
    // Key is not in cache -> use given extra
    assert!(path.ends_with("8.10"));

    let key4 = TestExtraMetadata { a: 5, b: 11 };
    let path = cache
        .get_path_or_insert_with_extra(&key4, with(vec![], 10))
        .await
        .unwrap();
    // Key is in cache -> use original extra
    assert!(path.ends_with("5.6"));
}

#[tokio::test]
async fn lock() {
    // Setup:
    // * Create a with a single pinned item taking all the space
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let mut cache = ProtonCache::<TestConfig>::new(dir.clone(), 20, vec![])
        .await
        .unwrap();
    let value = "A very big file".as_bytes(); // 15 bytes
    cache.add_item("key1".into(), value).unwrap();
    cache.lock_item("key1".into()).unwrap();

    // Action:
    // * Try to add another item that should evict first item
    cache.add_item("key2".into(), value).unwrap();

    // Validation:
    // * First item is still here
    // * Second item inserted even if cache exceed its size
    assert!(cache.get_item(&"key2".into()).unwrap().is_some());
    assert!(cache.get_item(&"key1".into()).unwrap().is_some());

    cache.unlock_item(&"key1".into());

    // Action:
    // * Try to add yet another item that should evict first item (and second)
    cache.add_item("key3".into(), value).unwrap();

    // Validation:
    // * First and second items are no more
    // * Third is here
    assert!(cache.get_item(&"key3".into()).unwrap().is_some());
    assert!(cache.get_item(&"key2".into()).unwrap().is_none());
    assert!(cache.get_item(&"key1".into()).unwrap().is_none());
}
