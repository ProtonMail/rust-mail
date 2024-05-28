use proton_core_common::cache::{CacheKey, ProtonCache};
use std::ffi::OsString;
use std::fs::{read_dir, File};
use std::io::Read;
use tempdir::TempDir;

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct TestKey(OsString);

impl CacheKey for TestKey {
    fn to_filename(&self) -> OsString {
        self.0.clone()
    }
}

fn get_content(mut file: impl Read) -> Vec<u8> {
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    content
}

#[test]
fn create_cache() {
    // Setup:
    //   * Create a temporary directory for cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();

    // Action 1:
    //   * Create the cache
    let _cache = ProtonCache::<TestKey>::new(dir.clone(), 1000).unwrap();

    // Validate:
    //   * Directory exist and is empty (+1 for cache data)
    let dir = read_dir(dir).unwrap();
    assert_eq!(dir.count(), 1);
}

#[test]
fn add_get_cache_item() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestKey>::new(dir.clone(), 1000).unwrap();
    let value = "A very big file".as_bytes();
    let key = TestKey("Key".into());

    // Actions:
    //   * Add an item into cache
    cache.add_item(key.clone(), value).unwrap();

    // Validation:
    //   * Item content is given value
    //   * An existing item have a path
    //   * An non-existing item have no path
    //   * There is a file on disk (+1 for cache data)
    let got = cache.get_item(&key).unwrap().unwrap();
    let path1 = cache.get_item_path(&key);
    let path2 = cache.get_item_path(&TestKey("Foo".into()));
    assert_eq!(value.to_vec(), get_content(got));
    assert!(path1.is_some());
    assert!(path2.is_none());
    let dir = read_dir(dir).unwrap();
    assert_eq!(dir.count(), 2);
    let file = cache.path_from_key(&key);
    let mut file = File::open(file).unwrap();
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    assert_eq!(content, value);
}

#[test]
fn add_item_twice() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestKey>::new(dir.clone(), 1000).unwrap();
    let value = "A very big file".as_bytes();
    let other_value = "Another very big file".as_bytes();
    let key = TestKey("Key".into());

    // Actions:
    //   * Add two different items with same key
    cache.add_item(key.clone(), value).unwrap();
    cache.add_item(key.clone(), other_value).unwrap();

    // Validation:
    //   * Only one file on disk (+1 for cache data)
    let dir = read_dir(dir).unwrap();
    assert_eq!(dir.count(), 2);
    let file = cache.path_from_key(&key);
    let mut file = File::open(file).unwrap();
    let mut content = Vec::new();
    file.read_to_end(&mut content).unwrap();
    assert_eq!(content, other_value);
}

#[test]
fn eviction() {
    // Setup:
    //   * Create a cache
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestKey>::new(dir.clone(), 100).unwrap();
    let value = "A very big file".as_bytes(); // 15 bytes
    let to_create = 100;

    // Actions:
    //   * Add many items
    for i in 0..to_create {
        cache
            .add_item(TestKey(format!("{i}").into()), value)
            .unwrap();
    }

    // Validation:
    //   * Only a few items are still in cache
    let dir = read_dir(dir).unwrap();
    let file_count = dir.count();
    let cache_count = cache.len();
    assert_eq!(file_count, cache_count + 1);
    assert_eq!(cache_count, 6); // (6+1) * 15 = 105 => maximum 6 values of 15 bytes
}

#[test]
fn remove() {
    // Setup:
    //   * Create a cache with a value
    let dir = TempDir::new("test").unwrap();
    let dir = dir.into_path();
    let cache = ProtonCache::<TestKey>::new(dir.clone(), 1000).unwrap();
    let value = "A very big file".as_bytes(); // 15 bytes
    cache.add_item(TestKey("key1".into()), value).unwrap();
    cache.add_item(TestKey("key2".into()), value).unwrap();
    cache.add_item(TestKey("key3".into()), value).unwrap();

    // Action:
    //   * Remove value from cache
    cache.remove(&TestKey("key2".into())).unwrap();

    // Validation:
    //   * The value is no more here
    let dir = read_dir(dir).unwrap();
    let file_count = dir.count();
    assert_eq!(cache.len(), 2);
    assert_eq!(file_count, 3);
}
