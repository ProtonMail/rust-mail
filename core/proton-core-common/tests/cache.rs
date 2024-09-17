use futures::future::join_all;
use proton_core_common::cache::{
    CacheConfig, CacheKey, CacheResult, ProtonCache, WeightingStrategy,
};
use std::ffi::OsString;
use std::fs::{read_dir, File};
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

    type Init = Vec<TestKey>;

    async fn get_existing(init: Vec<TestKey>) -> CacheResult<Vec<TestKey>> {
        Ok(init)
    }

    async fn handle_failed(_failed: Vec<TestKey>) -> CacheResult<()> {
        Ok(())
    }

    fn key_to_filename(key: &Self::Key) -> OsString {
        key.0.clone()
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct TestWeightlessKey;

impl CacheConfig for TestWeightlessKey {
    type Key = TestKey;
    type Init = Vec<TestKey>;

    async fn get_existing(init: Vec<TestKey>) -> CacheResult<Vec<TestKey>> {
        Ok(init)
    }

    async fn handle_failed(_failed: Vec<TestKey>) -> CacheResult<()> {
        Ok(())
    }

    fn key_to_filename(key: &Self::Key) -> OsString {
        key.0.clone()
    }

    fn weighting_strategy() -> WeightingStrategy {
        WeightingStrategy::Zero
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
    let file = cache.path_from_key(&key);
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
    let file = cache.path_from_key(&key);
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
