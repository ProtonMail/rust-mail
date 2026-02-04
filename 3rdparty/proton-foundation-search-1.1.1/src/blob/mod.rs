//! Blob handling code

use alloc::sync::Arc;
use core::any::{Any, type_name};
use core::fmt::{Debug, Display};
use core::ops::ControlFlow;
use core::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};

use once_cell::race::OnceBox;
use serde::{Deserialize, Serialize};
use tracing::trace;
use uuid::Uuid;

use crate::prelude::*;
use crate::serialization::SerDes;

#[cfg(feature = "wasm-bindgen")]
mod wasm;

#[cfg(test)]
mod tests;

/// Read-only access to a blob which is loaded on first request
/// through a [`LoadEvent`], saved with a [`SaveEvent`]
/// and released through a [`ReleaseEvent`]
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Blob<T: BlobValue> {
    /// A unique identifier of the blob
    id: BlobId,
    /// Human friendly description of the blob for debugging
    #[serde(skip)]
    description: Box<str>,
    /// Content and load_event_produced represent three states:
    ///  - None if load_event_produced == false - it hasn't been checked yet
    ///  - None if load_event_produced == true - waiting for a load
    ///  - Some(occupied) - loaded
    #[serde(skip)]
    content: Arc<OnceBox<Arc<T>>>,
    /// Flags that a load event has been generated
    #[serde(skip)]
    load_event_produced: AtomicBool,
}

impl<T: BlobValue> Clone for Blob<T> {
    /// Cloning a blob is tricky as it captures a transient state that may only occur once.
    /// A clone of a blob that has not been loaded yet will thus generate a new load event.
    fn clone(&self) -> Self {
        let (content, loading) = if self.content.get().is_some() {
            // The blob is already loaded, we can safely use that value.
            // Setting load_event_produced to true for consistency.
            (self.content.clone(), true.into())
        } else {
            Default::default()
        };

        Self {
            id: self.id,
            description: self.description.clone(),
            load_event_produced: loading,
            content,
        }
    }
}

impl<T: BlobValue> core::hash::Hash for Blob<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Trait for values that can be stored in a blob
pub trait BlobValue:
    'static + Sync + Send + Debug + Default + Serialize + for<'de> Deserialize<'de>
{
}

impl<T> BlobValue for T where
    T: 'static + Sync + Send + Debug + Default + Serialize + for<'de> Deserialize<'de>
{
}

impl<T: BlobValue> Blob<T> {
    /// Create a new blob with given ID.
    /// Description is used only for debugging.
    pub fn new(id: impl Into<BlobId>, description: impl Into<Box<str>>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            content: Default::default(),
            load_event_produced: Default::default(),
        }
    }

    /// The blob identificator
    pub fn id(&self) -> BlobId {
        self.id
    }

    /// Reset the blob state, freeing cached blob content from memory.
    /// Next load would the produce a load event again.
    pub fn free(&mut self) {
        self.content = Default::default();
        self.load_event_produced = Default::default();
    }

    /// Check if the blob is without content nor expecting any
    pub fn is_free(&self) -> bool {
        self.content.get().is_none()
    }

    /// Replace the blob content, generating a release and save event.
    pub fn save(&mut self, new_id: BlobId, value: Arc<T>) -> (Option<ReleaseEvent>, SaveEvent) {
        (
            (new_id != self.id)
                .then_some(ReleaseEvent::new(core::mem::replace(&mut self.id, new_id))),
            SaveEvent::create(self.id, self.description.clone(), self.set(value).clone()),
        )
    }

    /// Replace the blob content, generating a release and save event. Free it immediately.
    pub fn save_and_free(
        &mut self,
        new_id: BlobId,
        value: Arc<T>,
    ) -> (Option<ReleaseEvent>, SaveEvent) {
        self.free();
        (
            (new_id != self.id)
                .then_some(ReleaseEvent::new(core::mem::replace(&mut self.id, new_id))),
            SaveEvent::create(self.id, self.description.clone(), value),
        )
    }

    /// Get the content, or load it.
    ///
    /// The returned LoadEvent must be handled before calling load again or else we panic.
    /// This ensures there is no ambiguity about dropped LoadEvents:
    ///   - shall the engine proceed with default (incomplete results)?
    ///   - shall it ask to load again (endless loop)?
    ///   - shall it stop processing as there is likely nobody interested in the result?
    pub fn load(&self) -> ControlFlow<LoadEvent, &Arc<T>> {
        if let Some(v) = self.content.get() {
            ControlFlow::Continue(v)
        } else {
            if self.load_event_produced.swap(true, Ordering::AcqRel) {
                panic!(
                    "load event was not handled for blob id: {:?}, desc: {:?}",
                    self.id, self.description
                );
            }
            let event = LoadEvent::create_with_oncebox(
                self.id,
                self.description.clone(),
                self.content.clone(),
            );
            ControlFlow::Break(event)
        }
    }

    /// Load the blob content and free it immediately
    #[cfg_attr(feature = "bench-mode", inline(never))]
    pub fn fetch_and_free(&mut self) -> ControlFlow<LoadEvent, Arc<T>> {
        trace!(method="fetch_and_free",
            ?self.id,
            ?self.description,
            ?self.content
        );
        let flow = ControlFlow::Continue(self.load()?.clone());
        self.free();
        flow
    }

    #[allow(clippy::expect_used)]
    fn set(&mut self, value: Arc<T>) -> &Arc<T> {
        self.content = Arc::new(OnceBox::new());
        self.content
            .set(Box::new(value))
            .expect("new OnceBox created here should not be occupied");
        // NOTE: We just set it, so get() will return Some
        self.content.get().expect("value was just set")
    }

    /// Get the debugging description of the blob
    pub fn description(&self) -> &str {
        self.description.as_ref()
    }
}

/// A wrapper type handed out to application to be cached.
/// It can then be used to handle subsequent load events.
#[derive(Clone)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Cached {
    description: Box<str>,
    type_name: &'static str,
    data: Arc<dyn Any + Sync + Send + 'static>,
    serialize_fn: SaveCallback,
}

impl Cached {
    /// Create a new cached value
    pub fn new<T: Any + Sync + Send + Debug + Serialize + 'static>(
        description: impl Into<Box<str>>,
        data: Arc<T>,
    ) -> Self {
        Self {
            type_name: type_name::<T>(),
            description: description.into(),
            data: data.clone(),
            serialize_fn: Arc::new(move |serdes| serdes.serialize(&*data)),
        }
    }

    /// Serialize the cached data
    pub fn serialize(&self, serdes: &SerDes) -> Result<Vec<u8>, BoxError> {
        (self.serialize_fn)(serdes)
    }

    /// Get the contained value
    pub(crate) fn downcast<T: Send + Sync + 'static>(
        &self,
        description: impl Into<Box<str>>,
    ) -> Result<Arc<T>, InvalidCast> {
        if let Ok(value) = Arc::downcast::<T>(self.data.clone()) {
            Ok(value.clone())
        } else {
            Err(InvalidCast {
                want: type_name::<T>(),
                want_description: description.into(),
                got: self.type_name,
                got_description: self.description.clone(),
            })
        }
    }
}

impl Display for Cached {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} - cached {}", self.description, self.type_name)
    }
}

impl Debug for Cached {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Cached")
            .field("description", &self.description)
            .field("type_name", &self.type_name)
            .finish_non_exhaustive()
    }
}

/// Load event handling variants
#[derive(Debug)]
pub enum Load<'a> {
    /// The value was loaded from a cache
    Cached(Cached),
    /// The value shall be deserialized from storage
    Serialized(&'a SerDes, &'a [u8]),
}

/// Asks the app to load a blob.
///
/// The blob can be provided in serialized form, calling `send()`.
/// Or it can be provided from cache, calling `send_cached()`.
/// Or if the value is neither cached nor stored, calling `send_empty()`.
///
/// Panics: It is an error not to call any of the send* methods before further polling
/// the engine state machine producing it. Doing so, the next poll of `next()` will panic.
pub struct LoadEvent {
    id: BlobId,
    description: Box<str>,
    call: LoadCallback,
}

impl LoadEvent {
    /// Create a new load event with a callback
    pub fn new(id: BlobId, description: Box<str>, call: LoadCallback) -> Self {
        Self {
            id,
            description,
            call,
        }
    }

    /// Create a new LoadEvent.
    /// The generated callback handles both cached and serialized variants.
    pub fn create<T: BlobValue>(
        name: impl Into<BlobId>,
        description: impl Into<Box<str>>,
    ) -> (Self, Arc<OnceBox<Arc<T>>>) {
        let oncebox: Arc<OnceBox<Arc<T>>> = Arc::new(OnceBox::new());
        (
            Self::create_with_oncebox(name, description, oncebox.clone()),
            oncebox,
        )
    }
    /// Create a new LoadEvent.
    /// The generated callback handles both cached and serialized variants.
    pub fn create_with_oncebox<T: BlobValue>(
        name: impl Into<BlobId>,
        description: impl Into<Box<str>>,
        oncebox: Arc<OnceBox<Arc<T>>>,
    ) -> Self {
        let id = name.into();
        let description = description.into();

        Self {
            id,
            description: description.clone(),
            call: Box::new(move |data: Option<Load<'_>>| {
                let data: Arc<T> = match data {
                    None => Arc::new(T::default()),
                    Some(Load::Cached(cached)) => cached.downcast(description.as_ref())?,
                    Some(Load::Serialized(serdes, data)) => Arc::new(serdes.deserialize(data)?),
                };

                #[allow(
                    clippy::expect_used,
                    reason = "This is a FnOnce, set will only be called once"
                )]
                oncebox.set(Box::new(data.clone())).expect("set only once");

                Ok(Cached::new(description, data))
            }),
        }
    }

    /// Get the blob name
    pub fn id(&self) -> BlobId {
        self.id
    }

    /// Send serialized data.
    ///
    /// Sending an empty buffer is an error. Use `send_empty()` instead.
    pub fn send(self, serdes: &SerDes, data: &[u8]) -> Result<Cached, BoxError> {
        trace!(msg = "deserializing", data = ?String::from_utf8_lossy(data));
        (self.call)(Some(Load::Serialized(serdes, data)))
    }

    /// Send data fetched from memory cache.
    ///
    /// Obviously, the contained Any type must be the same as previously cached for this name,
    /// otherwise an error is produced.
    pub fn send_cached(self, cached: &Cached) -> Result<(), BoxError> {
        (self.call)(Some(Load::Cached(cached.clone())))?;
        Ok(())
    }

    /// If not cached nor serialized, inform the engine to proceed with default
    /// It is an error not to resolve a load event (to avoid ambiguity).
    pub fn send_empty(self) -> Result<Cached, BoxError> {
        (self.call)(None)
    }
}
impl Debug for LoadEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LoadEvent")
            .field("name", &self.id)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

/// Asks the app to save a blob in cache and/or storage.
pub struct SaveEvent {
    id: BlobId,
    cached: Cached,
}

impl SaveEvent {
    /// Create a new save event with a callback
    pub fn new(id: BlobId, cached: Cached) -> Self {
        Self { id, cached }
    }

    /// Create a new save event.
    ///
    /// The generated callback serializes the value using SerDes provided by the app.
    pub fn create<T: BlobValue>(
        id: impl Into<BlobId>,
        description: impl Into<Box<str>>,
        value: Arc<T>,
    ) -> Self {
        let subject = value.clone();
        let description = description.into();
        let cached = Cached::new(description, subject);
        Self::new(id.into(), cached)
    }

    /// Get the blob ID
    pub fn id(&self) -> BlobId {
        self.id
    }

    /// Get the cached value to create an in-memory only engine,
    /// or call serialize(SerDes) on it to store it.
    pub fn recv(self) -> Cached {
        self.cached
    }
}

impl Debug for SaveEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SaveEvent")
            .field("name", &self.id)
            .field("cached", &self.cached)
            .finish_non_exhaustive()
    }
}

/// Represents a completed operation
#[derive(Debug)]
pub struct RevisionEvent(BlobId);

impl RevisionEvent {
    /// Create a new revision event
    pub fn new(revision: BlobId) -> Self {
        Self(revision)
    }

    /// Get the revision ID
    pub fn id(&self) -> BlobId {
        self.0
    }
}

/// Represents a blob no longer needed by given revision
#[derive(Debug)]
pub struct ReleaseEvent(BlobId);

impl ReleaseEvent {
    /// Create a new release event
    pub fn new(released: BlobId) -> Self {
        Self(released)
    }

    /// Get the released blob ID
    pub fn id(&self) -> BlobId {
        self.0
    }
}

/// Call to serialize a blob
pub type SaveCallback = Arc<dyn Send + Sync + Fn(&SerDes) -> Result<Vec<u8>, BoxError>>;

/// Call to load a blob
pub type LoadCallback = Box<dyn Send + Sync + FnOnce(Option<Load<'_>>) -> Result<Cached, BoxError>>;

/// Error returned on wrong cached value
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidCast {
    want: &'static str,
    want_description: Box<str>,
    got: &'static str,
    got_description: Box<str>,
}
impl core::error::Error for InvalidCast {}
impl core::fmt::Display for InvalidCast {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Invalid cast, wanted {:?} ({:?}), got {:?} ({:?})",
            self.want, self.want_description, self.got, self.got_description
        )
    }
}

/// Unique blob identifier
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct BlobId(u64, u64);

impl BlobId {
    pub(crate) const fn new(a: u64, b: u64) -> Self {
        Self(a, b)
    }

    /// Generate random ID for a blob.
    /// In test, ID is derived crom content hash to make it stable.
    #[cfg_attr(feature = "bench-mode", inline(never))]
    pub fn generate(content: impl core::hash::Hash) -> Self {
        #[cfg(any(test, feature = "test-stable-blob-id"))]
        {
            #![allow(deprecated)]
            use core::hash::{Hasher, SipHasher};
            // for test, generate predictable IDs
            let mut state = SipHasher::new_with_keys(42, 108);
            content.hash(&mut state);
            let hash = state.finish();
            uuid::Uuid::new_v5(&uuid::Uuid::default(), &hash.to_be_bytes()).into()
        }

        #[cfg(not(any(test, feature = "test-stable-blob-id")))]
        {
            let _ = content;
            uuid::Uuid::new_v4().into()
        }
    }
}

impl From<Uuid> for BlobId {
    fn from(value: Uuid) -> Self {
        let (a, b) = value.as_u64_pair();
        BlobId(a, b)
    }
}
impl From<BlobId> for Uuid {
    fn from(value: BlobId) -> Self {
        Uuid::from_u64_pair(value.0, value.1)
    }
}
impl Debug for BlobId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self, f)
    }
}
impl Display for BlobId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self(a, b) = self;
        write!(f, "{a:016X}{b:016X}")
    }
}
impl FromStr for BlobId {
    type Err = InvalidBlobId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(InvalidBlobId);
        }
        if s.len() != 32 {
            return Err(InvalidBlobId);
        }
        let (a, b) = s.split_at_checked(16).ok_or(InvalidBlobId)?;
        let a = u64::from_str_radix(a, 16).map_err(|_| InvalidBlobId)?;
        let b = u64::from_str_radix(b, 16).map_err(|_| InvalidBlobId)?;
        Ok(Self(a, b))
    }
}

/// Error returned when a blob ID cannot be parsed
#[derive(Debug)]
pub struct InvalidBlobId;
impl core::error::Error for InvalidBlobId {}
impl Display for InvalidBlobId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid blob ID")
    }
}

impl Serialize for BlobId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BlobId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        Box::<str>::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}
