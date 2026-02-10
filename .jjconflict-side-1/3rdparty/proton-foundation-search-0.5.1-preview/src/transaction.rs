//! Reusable blob loading and saving implementations and strategies

use std::any::type_name;
use std::borrow::Borrow;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use arc_swap::ArcSwapOption;
use serde::{Deserialize, Serialize};
use tracing::{info, trace, warn};

use crate::serialization::SerDes;

/// Store and search load callback event
pub struct LoadEvent {
    /// blob name to load
    pub name: Box<str>,
    /// blob content callback
    pub send: LoadCallback,
}
impl LoadEvent {
    /// Handle the load event with an empty blob
    pub fn send_empty(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        (self.send)(&Default::default(), vec![])
    }
}
impl Debug for LoadEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadEvent")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Store save callback event
pub struct SaveEvent {
    /// blob name to save
    pub name: Box<str>,
    /// blob content callback
    pub recv: SaveCallback,
}
impl Debug for SaveEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveEvent")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Release obsolete blob (to delete)
#[derive(Debug)]
pub struct ReleaseEvent {
    /// blob name to release
    pub name: Box<str>,
}

/// Store and search load callback
pub type LoadCallback = Box<
    dyn Send
        + Sync
        + FnOnce(&SerDes, Vec<u8>) -> Result<(), Box<dyn Send + Sync + std::error::Error>>,
>;

/// Store save callback
pub type SaveCallback = Box<
    dyn Send + Sync + FnOnce(&SerDes) -> Result<Vec<u8>, Box<dyn Send + Sync + std::error::Error>>,
>;

/// Helper struct performs common loading/saving operations in a transaction state machine
pub struct TransactionState<I, C> {
    name: Box<str>,
    loading: Option<Receiver<(u64, C)>>,
    content: I,
    revision: u64,
    default: Arc<dyn Send + Sync + Fn() -> C>,
}

impl<I: Debug, C> Debug for TransactionState<I, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionState")
            .field("name", &self.name)
            .field("loading", &self.loading)
            .field("content", &self.content)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl<C> TransactionState<Read<C>, C>
where
    C: for<'de> Deserialize<'de> + Default + Send + 'static,
    Read<C>: CacheTransaction<C>,
{
    /// start a new transaction with shared reference output
    pub fn read(
        revision: u64,
        name: Box<str>,
        written: Option<Arc<(u64, C)>>,
        target: Arc<ArcSwapOption<(u64, C)>>,
    ) -> Self {
        Self::new(
            name,
            revision,
            Read::new(revision, written, target),
            C::default,
        )
    }
}

impl<C> TransactionState<StaticRead<C>, C>
where
    C: for<'de> Deserialize<'de> + Default + Send + 'static,
    StaticRead<C>: CacheTransaction<C>,
{
    /// start a new transaction with shared reference output
    pub fn static_read(
        revision: u64,
        name: Box<str>,
        written: Option<Arc<(u64, C)>>,
        target: Arc<ArcSwapOption<(u64, C)>>,
    ) -> Self {
        Self::new(
            name,
            revision,
            StaticRead::new(revision, written, target),
            C::default,
        )
    }
}

impl<C> TransactionState<Write<C>, C>
where
    C: for<'de> Deserialize<'de> + Default + Send + 'static,
    Write<C>: CacheTransaction<C>,
{
    /// start a new transaction with a mutable reference output
    pub fn write(
        revision: u64,
        name: Box<str>,
        cached: Option<Arc<(u64, C)>>,
        target: Arc<ArcSwapOption<(u64, C)>>,
    ) -> Self {
        Self::new(
            name,
            revision,
            Write::new(revision, cached, target),
            C::default,
        )
    }
}

impl<C> TransactionState<NoCache<C>, C>
where
    C: for<'de> Deserialize<'de> + Send + 'static,
    NoCache<C>: CacheTransaction<C>,
{
    /// start a new transaction with a manifest - no revisions, no cache
    pub fn no_cache(name: Box<str>, default: impl 'static + Send + Sync + Fn() -> C) -> Self {
        Self::new(name, 0, NoCache::new(), default)
    }
}

impl<I, C> TransactionState<I, C>
where
    I: CacheTransaction<C>,
    C: for<'de> Deserialize<'de> + Send + 'static,
{
    /// Create a new TransactionState in its initial state
    pub fn new(
        name: Box<str>,
        revision: u64,
        content: I,
        default: impl 'static + Send + Sync + Fn() -> C,
    ) -> Self {
        Self {
            name,
            revision,
            loading: None,
            content,
            default: Arc::new(default),
        }
    }

    /// Advance state machine in loading current content.
    /// None => loading failed, stop machine
    /// Some(Ok) => value loaded
    /// Some(Err) => load request
    #[allow(clippy::type_complexity)]
    pub fn load(&mut self) -> Option<Result<I::Ref<'_>, LoadEvent>> {
        let name_with_revision = Self::revision_name(&self.name, self.revision);
        trace!("loading {name_with_revision}");

        if let Some(receiver) = &self.loading {
            trace!("receiving content {name_with_revision:?}");
            // Attempt to load only once.
            // The caller must ensure that loaded data is sent before polling next again.
            let (loaded_revision, loaded_index) = receiver.try_recv().ok()?;
            assert_eq!(
                loaded_revision, self.revision,
                "The loaded r{loaded_revision} from {name_with_revision:?} does not match expected revision r{}.",
                self.revision
            );
            trace!("received content {name_with_revision:?}");
            self.loading = None;
            self.content.set(loaded_revision, loaded_index);
        }

        let Some(cached) = self.content.get(self.revision) else {
            let default = self.default.clone();
            let (rx, send) = load(move || default());
            self.loading = Some(rx);
            trace!("requesting content {name_with_revision:?}");

            return Some(Err(LoadEvent {
                name: name_with_revision,
                send,
            }));
        };

        trace!("got content {name_with_revision:?}");

        Some(Ok(cached))
    }

    pub(crate) fn reset(self) -> ReleaseEvent
    where
        C: Serialize + Sync,
        I: WritableCacheTransaction<C>,
    {
        self.content.unset();

        let name = Self::revision_name(&self.name, self.revision);
        ReleaseEvent { name }
    }

    pub(crate) fn save(mut self) -> (SaveEvent, Option<ReleaseEvent>)
    where
        C: Debug + Serialize + Sync,
        I: WritableCacheTransaction<C>,
    {
        let last_name_with_revision = Self::revision_name(&self.name, self.revision);
        let content = self.content.finish(self.revision);
        self.revision = content
            .as_deref()
            .map(|(rev, _)| *rev)
            .unwrap_or(self.revision);
        let name_with_revision = Self::revision_name(self.name, self.revision);

        info!("saving content {name_with_revision:?}, releasing {last_name_with_revision:?}");

        let released = (last_name_with_revision != name_with_revision).then_some(ReleaseEvent {
            name: last_name_with_revision,
        });

        let saved = SaveEvent {
            name: name_with_revision,
            recv: Box::new(move |serdes: &SerDes| match content.as_deref() {
                Some((revision, content)) => serdes.serialize(&(revision, content)),
                None => Ok(vec![]),
            }),
        };

        (saved, released)
    }

    fn revision_name(name: impl AsRef<str>, revision: u64) -> Box<str> {
        [name.as_ref(), " r", &revision.to_string()]
            .concat()
            .into_boxed_str()
    }
}

fn load<C>(default: impl 'static + Sync + Send + Fn() -> C) -> (Receiver<(u64, C)>, LoadCallback)
where
    C: 'static + for<'de> Deserialize<'de> + Send,
{
    // revisions do not match, reload data
    let (tx, rx) = std::sync::mpsc::channel();
    let send = Box::new(move |serdes: &SerDes, data: Vec<u8>| {
        let index = if data.is_empty() {
            (0, default())
        } else {
            serdes.deserialize(data.as_slice())?
        };
        tx.send(index).unwrap_or_else(|_e| {
            warn!(
                "The other side is no longer awaiting a response on load. We will not care either."
            )
        });
        Ok(())
    });
    (rx, send)
}

/// Helper trait enables us to treat read and write transactions the same
/// returning either a readonly ref/arc or a mutable reference
pub trait CacheTransaction<C> {
    /// borrow is used internally in [`TransactionState`]
    type Ref<'r>: Borrow<C>
    where
        Self: 'r;
    /// Get content, either mutable or shared ref/Arc
    fn get(&mut self, revision: u64) -> Option<Self::Ref<'_>>;
    /// Get content to a new value
    fn set(&mut self, revision: u64, content: C);
}

/// Helper trait extending the [`CacheTransaction`] enables us to handle write transactions
pub trait WritableCacheTransaction<C>: CacheTransaction<C> {
    /// finalize the transaction, consuming it
    fn finish(self, revision: u64) -> Option<Arc<(u64, C)>>;
    /// clear the cache in case of reset
    fn unset(self);
}

/// Cache state in a write transaction
/// Most likely, the current read state would be up to date,
/// saving us an IO reload on write
#[derive(Debug)]
pub struct Write<C> {
    cached: Option<Arc<(u64, C)>>,
    owned: Option<(u64, C)>,
    target: Arc<ArcSwapOption<(u64, C)>>,
}

impl<C> Write<C> {
    /// Create content from cached value
    pub fn new(
        revision: u64,
        cached: Option<Arc<(u64, C)>>,
        target: Arc<ArcSwapOption<(u64, C)>>,
    ) -> Self {
        Self {
            cached: cached_or_target(revision, cached, &target),
            target,
            owned: None,
        }
    }
}

impl<C: Clone> CacheTransaction<C> for Write<C> {
    type Ref<'r>
        = &'r mut C
    where
        Self: 'r;

    fn get(&mut self, revision: u64) -> Option<Self::Ref<'_>> {
        if self.owned.is_none() {
            self.owned = self.cached.as_deref().cloned();
        }
        let (rev, owned) = self.owned.as_mut()?;
        if *rev == revision { Some(owned) } else { None }
    }

    fn set(&mut self, revision: u64, value: C) {
        self.owned = Some((revision, value))
    }
}
impl<C: Clone> WritableCacheTransaction<C> for Write<C> {
    fn finish(mut self, revision: u64) -> Option<Arc<(u64, C)>> {
        if self.owned.is_none() {
            self.owned = self.cached.as_deref().cloned();
        }
        let revision = revision.wrapping_add(1);
        if let Some((r, _)) = &mut self.owned {
            *r = revision
        }

        match self.owned.take() {
            Some(owned) => {
                trace!("updating target rev {} for {:?}", owned.0, type_name::<C>());
                let bundle = Some(Arc::new(owned));
                self.target.store(bundle.clone());
                bundle
            }
            None => self.cached.clone().or_else(|| self.target.load_full()),
        }
    }

    fn unset(self) {
        self.target.store(None);
    }
}

/// Accessor for manifest - no revision, no cache
#[derive(Debug)]
pub struct NoCache<C> {
    owned: Option<C>,
}

impl<C> Default for NoCache<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> NoCache<C> {
    /// Create content from cached value
    pub fn new() -> Self {
        Self { owned: None }
    }
}

impl<C> CacheTransaction<C> for NoCache<C> {
    type Ref<'r>
        = &'r mut C
    where
        Self: 'r;

    fn get(&mut self, revision: u64) -> Option<Self::Ref<'_>> {
        assert_eq!(revision, 0);
        self.owned.as_mut()
    }

    fn set(&mut self, revision: u64, value: C) {
        assert_eq!(revision, 0);
        self.owned = Some(value)
    }
}
impl<C> WritableCacheTransaction<C> for NoCache<C> {
    fn finish(self, revision: u64) -> Option<Arc<(u64, C)>> {
        assert_eq!(revision, 0);
        self.owned.map(|owned| Arc::new((0, owned)))
    }

    fn unset(self) {
        // no-op
    }
}

/// Cache accessor that yields shared reference to the data
pub struct Read<C> {
    target: Arc<ArcSwapOption<(u64, C)>>,
    current: Option<Arc<(u64, C)>>,
}

impl<C> Read<C> {
    fn new(
        revision: u64,
        written: Option<Arc<(u64, C)>>,
        target: Arc<ArcSwapOption<(u64, C)>>,
    ) -> Self {
        Self {
            current: cached_or_target(revision, written, &target),
            target,
        }
    }
}

impl<C> CacheTransaction<C> for Read<C> {
    type Ref<'r>
        = &'r C
    where
        Self: 'r;

    fn get(&mut self, revision: u64) -> Option<Self::Ref<'_>> {
        self.current
            .as_ref()
            .filter(|curr| curr.0 == revision)
            .map(|current| &current.1)
    }

    fn set(&mut self, revision: u64, value: C) {
        self.current = Some(Arc::new((revision, value)));
        self.target.store(self.current.clone());
    }
}

/// Cache accessor that yields static Arc ref to the data
pub struct StaticRead<C> {
    target: Arc<ArcSwapOption<(u64, C)>>,
    current: Option<Arc<(u64, C)>>,
}

impl<C> StaticRead<C> {
    fn new(
        revision: u64,
        written: Option<Arc<(u64, C)>>,
        target: Arc<ArcSwapOption<(u64, C)>>,
    ) -> Self {
        Self {
            current: cached_or_target(revision, written, &target),
            target,
        }
    }
}

impl<C> CacheTransaction<C> for StaticRead<C> {
    type Ref<'r>
        = Cached<C>
    where
        Self: 'r;

    fn get(&mut self, revision: u64) -> Option<Self::Ref<'_>> {
        self.current
            .clone()
            .filter(|curr| curr.0 == revision)
            .map(|loaded| Cached(loaded))
    }

    fn set(&mut self, revision: u64, value: C) {
        self.current = Some(Arc::new((revision, value)));
        self.target.store(self.current.clone());
    }
}

/// if the cache is up to date, use it
/// or else use the target cache
fn cached_or_target<C>(
    revision: u64,
    cached: Option<Arc<(u64, C)>>,
    target: &ArcSwapOption<(u64, C)>,
) -> Option<Arc<(u64, C)>> {
    match cached.as_deref() {
        Some((rev, _)) if *rev == revision => {
            trace!("rev {rev} matched with cache");
            cached
        }
        _ => {
            let res = target.load_full();
            match res.as_deref() {
                Some((rev, _)) => trace!("rev {revision} loaded from target: {rev}"),
                None => trace!("rev {revision} not cached at all"),
            }
            res
        }
    }
}

/// Wrapper for values returned from [`Arc<ArcSwapOption<(u64, C)>>`] cache transaction
#[derive(Debug, Clone)]
pub struct Cached<C>(Arc<(u64, C)>);
impl<C> Cached<C> {
    #[allow(dead_code)]
    pub(crate) fn new(content: Arc<(u64, C)>) -> Self {
        Self(content)
    }
}
impl<C> From<Cached<C>> for Arc<(u64, C)> {
    fn from(value: Cached<C>) -> Self {
        value.0
    }
}
impl<C> Borrow<C> for Cached<C> {
    fn borrow(&self) -> &C {
        &self.0.1
    }
}
impl<C> AsRef<C> for Cached<C> {
    fn as_ref(&self) -> &C {
        &self.0.1
    }
}
impl<C> Deref for Cached<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.0.1
    }
}

//type Transaction<'a, V> = concread::arcache::ARCacheWriteTxn<'a, (), (u64, Index<V>), ()>;
// impl<'a, T: Clone + Debug + Send + Sync> CacheTransaction<T>
//     for concread::arcache::ARCacheWriteTxn<'a, (), (u64, T), ()>
// {
//     type Ref<'r>
//         = &'r mut (u64, T)
//     where
//         Self: 'r;

//     fn get(&mut self) -> Option<Self::Ref<'_>> {
//         self.get_mut(&(), false)
//     }

//     fn set(&mut self, revision: u64, value: T) {
//         self.insert((), (revision, value));
//     }

//     fn complete(self) {
//         self.commit()
//     }
// }

//type Transaction<'a, V> = concread::arcache::ARCacheReadTxn<'a, (), (u64, Index<V>), ()>;
// impl<'a, T: Clone + Debug + Send + Sync> CacheTransaction<T>
//     for concread::arcache::ARCacheReadTxn<'a, (), (u64, T), ()>
// {
//     type Ref<'r>
//         = &'r (u64, T)
//     where
//         Self: 'r;

//     fn get(&mut self) -> Option<Self::Ref<'_>> {
//         self.get(&())
//     }

//     fn set(&mut self, revision: u64, value: T) {
//         self.insert((), (revision, value));
//     }

//     fn complete(self) {
//         self.finish()
//     }
// }

#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use arc_swap::ArcSwapAny;

    use super::*;

    #[test]
    fn writer_updates_cache() {
        let cache = Arc::new(ArcSwapOption::new(Some(Arc::new((2, "initial")))));
        let mut sut = Write::new(3, cache.load_full(), cache.clone());

        match sut.get(1) {
            None => {}
            other => panic!("unexpected {other:?}"),
        }

        match sut.get(2) {
            Some(content) if *content == "initial" => {}
            other => panic!("unexpected {other:?}"),
        }

        sut.set(4, "fourth");
        match sut.get(4) {
            Some(content) if *content == "fourth" => {}
            other => panic!("unexpected {other:?}"),
        }

        match sut.get(3) {
            None => {}
            other => panic!("unexpected {other:?}"),
        }

        let result = sut.finish(3);
        assert_eq!(result, Some(Arc::new((4, "fourth"))));

        assert_eq!(cache.load_full(), Some(Arc::new((4, "fourth"))));
    }

    #[test]
    fn transaction_loads_rev0() {
        let mut sut = TransactionState::new(
            "test".into(),
            0,
            Write::<bool>::new(0, None, Default::default()),
            Default::default,
        );

        match sut.load() {
            Some(Err(LoadEvent { name, send })) if name.as_ref() == "test r0" => {
                send(&SerDes::Json, vec![]).expect("send")
            }
            next => panic!("unexpected {next:?}"),
        }

        match sut.load() {
            Some(Ok(false)) => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn transaction_loads_rev3() {
        let mut sut = TransactionState::new(
            "test".into(),
            3,
            Write::<bool>::new(3, None, Default::default()),
            Default::default,
        );

        match sut.load() {
            Some(Err(LoadEvent { name, send })) if name.as_ref() == "test r3" => {
                send(&SerDes::Json, br#"[3,true]"#.to_vec()).expect("send")
            }
            next => panic!("unexpected {next:?}"),
        }

        match sut.load() {
            Some(Ok(true)) => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn transaction_saves_rev0() {
        let mut sut = TransactionState::new(
            "test".into(),
            0,
            Write::<bool>::new(0, None, Default::default()),
            Default::default,
        );

        let LoadEvent { name, send } = sut.load().expect("some").expect_err("load");
        assert_eq!(name, "test r0".into());
        send(&SerDes::Json, vec![]).expect("send");

        let value = sut.load().expect("some").expect("value");
        assert_eq!(value, &mut false);
        *value = true;

        match sut.save() {
            (SaveEvent { name, recv }, Some(ReleaseEvent { name: released }))
                if name.as_ref() == "test r1" && released.as_ref() == "test r0" =>
            {
                assert_eq!(recv(&SerDes::Json).expect("recv"), b"[1,true]".to_vec());
            }
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn transaction_saves_rev15() {
        let cache = Arc::new(ArcSwapOption::default());
        let sut = TransactionState::new(
            "test".into(),
            15,
            Write::<bool>::new(15, Some(Arc::new((15, false))), cache.clone()),
            Default::default,
        );

        let res = match sut.save() {
            (SaveEvent { name, recv }, Some(ReleaseEvent { name: releaed }))
                if name.as_ref() == "test r16" && releaed.as_ref() == "test r15" =>
            {
                recv(&SerDes::Json).map(String::from_utf8)
            }
            next => panic!("unexpected {next:?}"),
        };

        assert_eq!(res.unwrap().unwrap(), "[16,false]");
        assert_eq!(cache.load_full(), Some(Arc::new((16, false))));
    }

    #[test]
    fn content_gets_none() {
        let mut sut = Write::<bool>::new(0, None, Default::default());

        match sut.get(0) {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn content_gets_cached() {
        let mut sut = Write::<bool>::new(5, Some(Arc::new((5, true))), Default::default());

        match sut.get(5) {
            Some(true) => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn content_gets_target() {
        let mut sut = Write::<bool>::new(
            5,
            None,
            Arc::new(ArcSwapAny::new(Some(Arc::new((5, true))))),
        );

        match sut.get(5) {
            Some(true) => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn content_gets_cache_before_target() {
        let mut sut = Write::<bool>::new(
            5,
            Some(Arc::new((5, true))),
            Arc::new(ArcSwapAny::new(Some(Arc::new((5, false))))),
        );

        match sut.get(5) {
            Some(true) => {}
            next => panic!("unexpected {next:?}"),
        }

        match sut.get(6) {
            None => {}
            next => panic!("unexpected {next:?}"),
        }
    }

    #[test]
    fn revision_gets_set() {
        let sut = Write::<bool>::new(1, Some(Arc::new((1, true))), Default::default());

        match sut.finish(2).as_deref() {
            Some((3, true)) => {}
            finish => panic!("unexpected {finish:?}"),
        }
    }

    #[test]
    fn content_gets_set() {
        let mut sut = Write::<bool>::new(0, None, Default::default());

        sut.set(7, true);

        assert_eq!(sut.get(7), Some(&mut true));
    }

    #[test]
    fn manifest_loads_fresh_every_time() {
        let mut sut: TransactionState<NoCache<()>, ()> =
            TransactionState::new("manifest".into(), 0, NoCache::new(), Default::default);

        match sut.load() {
            Some(Err(LoadEvent { name, send })) if name.as_ref() == "manifest r0" => {
                send(&SerDes::Json, vec![]).expect("send")
            }
            other => panic!("unexpected {other:?}"),
        }

        match sut.load() {
            Some(Ok(())) => {}
            other => panic!("unexpected {other:?}"),
        }

        match sut.save() {
            (SaveEvent { name, recv: _ }, None) if name.as_ref() == "manifest r0" => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}
