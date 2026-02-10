use fixedbitset::FixedBitSet;
use flume::{Receiver, Sender, TryRecvError};
use parking_lot::RwLock;
use slotmap::{SlotMap, new_key_type};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use tracing::{debug, error};

new_key_type! {
    /// Handle for a [`TableObserver`].
    pub struct TableObserverHandle;
}

/// Utility type that removes an observer from a [`Watcher`] when the type is dropped.
///
/// The [`TableObserver`] will be removed with [`Watcher::remove_observer_deferred()`].
#[derive(Debug, Clone)]
pub struct DropRemoveTableObserverHandle {
    watcher: Weak<Watcher>,
    handle: TableObserverHandle,
}

impl DropRemoveTableObserverHandle {
    fn new(handle: TableObserverHandle, watcher: &Arc<Watcher>) -> Self {
        Self {
            watcher: Arc::downgrade(watcher),
            handle,
        }
    }

    /// Returns the handle of the table observer.
    #[must_use]
    pub fn handle(&self) -> TableObserverHandle {
        self.handle
    }

    /// Unsubscribe the observer immediately.
    ///
    /// This can be safely called multiple times, the observer
    /// is only unsubscribed once.
    ///
    /// # Errors
    ///
    /// Returns error if we can't communicate with Watcher or the
    /// removal of the observer failed.
    pub fn unsubscribe(&self) -> Result<(), Error> {
        if let Some(watcher) = self.watcher.upgrade() {
            watcher.remove_observer(self.handle)
        } else {
            Err(Error::Command)
        }
    }
}

impl Drop for DropRemoveTableObserverHandle {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.upgrade() {
            if watcher.remove_observer_deferred(self.handle).is_err() {
                error!("Failed to remove watcher from observer on drop");
            }
        }
    }
}

/// Defines an observer for a set of tables.
pub trait TableObserver: Send + Sync {
    /// Return the set of tables this observer is interested in.
    fn tables(&self) -> Vec<String>;

    /// When one or more of the tables return by [`Self::tables()`] is modified, this method
    /// will be invoked by the [`Watcher`].
    ///
    /// `tables` contains the set of tables that we modified.
    ///
    /// It is recommended that the implementation be as short as possible to not delay/block
    /// the execution of other observers.
    fn on_tables_changed(&self, tables: &BTreeSet<String>);
}

/// The [`Watcher`] is the hub where updates are published regarding tables that updated when
/// observing a connection.
///
/// All changes are published to a background thread which then notifies the respective
/// [`TableObserver`]s.
///
/// # Observing Tables
///
/// To be notified of changes, register an observer with [`Watcher::add_observer`].
///
/// The [`Watcher`] by itself does not automatically watch all tables. The observed tables
/// are driven by the tables defined by each [`TableObserver`].
///
/// A table can be observed by many [`TableObserver`]. When the last [`TableObserver`] is removed
/// for a given table, that table stops being tracked.
///
/// # Update Propagation
///
/// Every time a [`TableObserver`] is added or removed, the list of tracked tables is updated and
/// a counter is bumped. These changes are propagated to [State] instances when they sync their
/// state [`State::sync_tables()`](crate::connection::State::sync_tables).
///
/// Due to the nature of concurrent operations, it is possible that a connection on different
/// thread will miss the changes applied from adding/removing an observer on the current thread. On
/// the next sync this will be rectified.
///
/// If both operation happen on the same thread, everything will work as expected.
///
/// # Notifications
///
/// To notify the [`Watcher`] of changed tables, an instance of either [Connection] or
/// [State] needs to be used. Check each type for more information on how to use
/// it correctly.
///
/// # Remarks
///
/// The [`Watcher`] currently maintains a list of observed tables that is never pruned. It will
/// keep growing with every new table that is observed. If you have af fixed set of tables that
/// you watch on a regular basis this is not an issue. If you have a dynamic list of tables
/// deleted tables are currently not removed. To be addressed in the future.
///
///
/// [Connection]: `crate::connection::Connection`
/// [State]: `crate::connection::State`
pub struct Watcher {
    tables: RwLock<ObservedTables>,
    tables_version: AtomicU64,
    sender: Sender<Command>,
}

const WATCHER_CHANNEL_CAPACITY: usize = 24;

impl Watcher {
    /// Create a new instance of an in process tracker service.
    ///
    /// # Errors
    /// Returns error if the worker thread fails to spawn.
    pub fn new() -> Result<Arc<Self>, Error> {
        let (sender, receiver) = flume::bounded(WATCHER_CHANNEL_CAPACITY);
        let watcher = Arc::new(Self {
            tables: RwLock::new(ObservedTables::new()),
            tables_version: AtomicU64::new(0),
            sender,
        });

        let watcher_cloned = Arc::clone(&watcher);
        std::thread::Builder::new()
            .name("sqlite_watcher".into())
            .spawn(move || {
                Watcher::background_loop(receiver, &watcher_cloned);
            })
            .map_err(Error::Thread)?;

        Ok(watcher)
    }

    /// Register a new observer with a list of interested tables.
    ///
    /// This function returns a [`TableObserverHandle`] which can later be used to
    /// remove the current observer.
    ///
    /// # Errors
    ///
    /// Returns error if the command which adds the observer to the background thread
    /// could not be sent or the handle could not be retrieved.
    pub fn add_observer(
        &self,
        observer: Box<dyn TableObserver>,
    ) -> Result<TableObserverHandle, Error> {
        let (sender, receiver) = oneshot::channel();
        if self
            .sender
            .send(Command::AddObserver(observer, sender))
            .is_err()
        {
            error!("Failed to send add observer command");
            return Err(Error::Command);
        }

        let Ok(handle) = receiver.recv() else {
            error!("Failed to receive handle for new observer");
            return Err(Error::Command);
        };

        Ok(handle)
    }

    /// Same as [`Self::add_observer`], but returns a handle that removes the observer
    /// from this [`Watcher`] on drop.
    ///
    ///
    /// # Errors
    ///
    /// See [`Self::add_observer`] for more details.
    pub fn add_observer_with_drop_remove(
        self: &Arc<Self>,
        observer: Box<dyn TableObserver>,
    ) -> Result<DropRemoveTableObserverHandle, Error> {
        let handle = self.add_observer(observer)?;

        Ok(DropRemoveTableObserverHandle::new(handle, self))
    }

    /// Remove an observer via its `handle` without waiting for the operation to complete.
    ///
    /// The removal of observers is deferred to the background thread and will
    /// be executed as soon as possible.
    ///
    /// If you wish to wait for an observer to finish being removed from the list,
    /// you should use [`Self::remove_observer()`]
    ///
    /// # Errors
    ///
    /// Returns error if the command to remove the observer could not be sent.
    pub fn remove_observer_deferred(&self, handle: TableObserverHandle) -> Result<(), Error> {
        self.sender
            .send(Command::RemoveObserverDeferred(handle))
            .map_err(|_| Error::Command)
    }

    /// Remove an observer via its `handle` and wait for it to be removed.
    ///
    /// If you wish do not wish to wait for an observer to finish being removed from the list,
    /// you should use [`Self::remove_observer_deferred()`]
    ///
    /// # Errors
    ///
    /// Returns error if the command to remove the observer could not be sent or the reply
    /// could not be received.
    pub fn remove_observer(&self, handle: TableObserverHandle) -> Result<(), Error> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::RemoveObserver(handle, sender))
            .map_err(|_| Error::Command)?;

        receiver.recv().map_err(|_| {
            error!("Failed to receive reply for remove observer command");
            Error::Command
        })
    }

    pub(crate) fn publish_changes(&self, table_ids: FixedBitSet) {
        if self
            .sender
            .send(Command::PublishChanges(table_ids))
            .is_err()
        {
            error!("Watcher could not communicate with background thread");
        }
    }

    pub(crate) async fn publish_changes_async(&self, table_ids: FixedBitSet) {
        if self
            .sender
            .send_async(Command::PublishChanges(table_ids))
            .await
            .is_err()
        {
            error!("Watcher could not communicate with background thread");
        }
    }

    #[cfg(test)]
    pub(crate) fn get_table_id(&self, table: &str) -> Option<usize> {
        self.with_tables(|tables| tables.table_ids.get(table).cloned())
    }

    fn with_tables_mut(&self, f: impl (FnOnce(&mut ObservedTables))) {
        let mut accessor = self.tables.write();
        // Save counter to check for significant changes
        let prev_counter = accessor.counter;

        (f)(&mut accessor);

        // Significant changes were made.
        let cur_counter = accessor.counter;
        if prev_counter != cur_counter {
            self.tables_version.fetch_add(1, Ordering::Release);
        }
    }

    fn with_tables<R>(&self, f: impl (FnOnce(&ObservedTables) -> R)) -> R {
        let accessor = self.tables.read();
        (f)(&accessor)
    }

    /// The current version of the tracked tables state.
    pub(crate) fn tables_version(&self) -> u64 {
        self.tables_version.load(Ordering::Acquire)
    }

    /// Return the list of observed tables at this point in time.
    pub fn observed_tables(&self) -> Vec<String> {
        self.with_tables(|t| t.tables.clone())
    }

    pub(crate) fn calculate_sync_changes(
        &self,
        connection_state: &FixedBitSet,
    ) -> (FixedBitSet, Vec<ObservedTableOp>) {
        self.with_tables(|t| t.calculate_changes(connection_state))
    }
    #[tracing::instrument(level= tracing::Level::TRACE, skip(receiver, watcher))]
    fn background_loop(receiver: Receiver<Command>, watcher: &Watcher) {
        let mut worker = WatcherWorker::new();

        loop {
            debug_assert!(worker.add_observers.is_empty());
            debug_assert!(worker.remove_observers.is_empty());
            debug_assert!(worker.publish_changes.is_empty());

            let Ok(command) = receiver.recv() else {
                return;
            };

            worker.unpack_command(command);

            // try to read more commands if any are queued.
            loop {
                match receiver.try_recv() {
                    Ok(command) => {
                        worker.unpack_command(command);
                    }
                    Err(e) => match e {
                        TryRecvError::Empty => {
                            break;
                        }
                        TryRecvError::Disconnected => {
                            return;
                        }
                    },
                }
            }

            worker.tick(watcher);
        }
    }
}

/// Background watcher worker which responds to [`Command`]s;
struct WatcherWorker {
    observers: SlotMap<TableObserverHandle, ActiveObserver>,
    updated_tables: BTreeSet<String>,
    remove_observers: Vec<(TableObserverHandle, Option<oneshot::Sender<()>>)>,
    add_observers: Vec<(Box<dyn TableObserver>, oneshot::Sender<TableObserverHandle>)>,
    publish_changes: Vec<FixedBitSet>,
}

impl WatcherWorker {
    fn new() -> Self {
        Self {
            observers: SlotMap::with_capacity_and_key(4),
            updated_tables: BTreeSet::default(),
            remove_observers: vec![],
            add_observers: vec![],
            publish_changes: vec![],
        }
    }
    fn unpack_command(&mut self, command: Command) {
        match command {
            Command::AddObserver(o, r) => self.add_observers.push((o, r)),
            Command::RemoveObserver(h, r) => self.remove_observers.push((h, Some(r))),
            Command::RemoveObserverDeferred(h) => {
                self.remove_observers.push((h, None));
            }
            Command::PublishChanges(fixedbitset) => {
                self.publish_changes.push(fixedbitset);
            }
        }
    }

    fn tick(&mut self, watcher: &Watcher) {
        // Remove old observers,
        for (handle, reply) in self.remove_observers.drain(..) {
            if let Some(observer) = self.observers.remove(handle) {
                watcher.with_tables_mut(|tables| {
                    tables.untrack_tables(observer.tables.iter());
                });
            }

            if let Some(reply) = reply {
                if reply.send(()).is_err() {
                    error!("Failed to send reply for observer removal");
                }
            }
        }

        // Add new observers
        for (observer, reply) in self.add_observers.drain(..) {
            let active_observer = ActiveObserver::new(observer);
            watcher.with_tables_mut(|tables| {
                tables.track_tables(active_observer.tables.iter().cloned());
            });
            let handle = self.observers.insert(active_observer);
            if reply.send(handle).is_err() {
                error!("Failed to send reply back to caller, new observer will not be added");
                self.observers.remove(handle);
            }
        }

        // Combine and publish changes.
        self.updated_tables.clear();

        for table_ids in self.publish_changes.drain(..) {
            if table_ids.is_clear() {
                continue;
            }

            // resolve table names.
            watcher.with_tables(|observer_tables| {
                for idx in table_ids.ones() {
                    // Safeguard against some invalid index, just in case.
                    if let Some(name) = observer_tables.tables.get(idx).cloned() {
                        self.updated_tables.insert(name);
                    }
                }
            });
        }

        if !self.updated_tables.is_empty() {
            debug!("Changes detected on tables: {:?}", self.updated_tables);
            // publish changes;
            {
                for (_, active_observer) in &self.observers {
                    if self
                        .updated_tables
                        .intersection(&active_observer.tables)
                        .next()
                        .is_some()
                    {
                        active_observer
                            .observer
                            .on_tables_changed(&self.updated_tables);
                    }
                }
            }
        }
    }
}

struct ActiveObserver {
    observer: Box<dyn TableObserver>,
    tables: BTreeSet<String>,
}

impl ActiveObserver {
    fn new(observer: Box<dyn TableObserver>) -> ActiveObserver {
        let tables = BTreeSet::from_iter(observer.tables());
        Self { observer, tables }
    }
}

/// Commands send to the background thread.
enum Command {
    /// Add a new observer
    AddObserver(Box<dyn TableObserver>, oneshot::Sender<TableObserverHandle>),
    /// Remove an observer
    RemoveObserverDeferred(TableObserverHandle),
    /// Remove an observer and wait for the operation to finish.
    RemoveObserver(TableObserverHandle, oneshot::Sender<()>),
    /// Publish new changes
    PublishChanges(FixedBitSet),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to send or receive command to/from background thread")]
    Command,
    #[error("Failed to create background thread: {0}")]
    Thread(std::io::Error),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ObservedTableOp {
    Add(String, usize),
    Remove(String, usize),
}

/// Keeps track of all the observed tables.
///
/// Each table is assigned an unique value (index) which is then propagated to all the trackers
/// when they sync their state.
struct ObservedTables {
    /// Table names to index/id
    table_ids: BTreeMap<String, usize>,
    /// Table names by index/id
    tables: Vec<String>,
    /// Number of active observers for each table.
    num_observers: Vec<usize>,
    /// Version counter.
    counter: u64,
}

impl ObservedTables {
    fn new() -> Self {
        Self {
            table_ids: BTreeMap::new(),
            tables: Vec::with_capacity(8),
            num_observers: Vec::with_capacity(8),
            counter: 0,
        }
    }

    /// Add the `tables` to the list of tables that need to be observed.
    fn track_tables(&mut self, tables: impl Iterator<Item = String>) {
        let mut requires_version_bump = false;
        for table in tables {
            match self.table_ids.entry(table.clone()) {
                Entry::Vacant(v) => {
                    let id = self.num_observers.len();
                    self.tables.push(table.clone());
                    self.num_observers.push(1);
                    v.insert(id);
                    requires_version_bump = true;
                }
                Entry::Occupied(o) => {
                    let id = *o.get();
                    let current = self.num_observers[id];
                    if current == 0 {
                        // We should start following this table again. If it is not
                        // 0, we are already observing it.
                        requires_version_bump = true;
                    }
                    self.num_observers[*o.get()] = current + 1;
                }
            }
        }

        if requires_version_bump {
            self.counter = self.counter.saturating_add(1);
        }
    }

    /// Remove the `tables` from the list of tables that need to be observed.
    fn untrack_tables<'i>(&mut self, tables: impl Iterator<Item = &'i String>) {
        let mut requires_version_bump = false;
        for table in tables {
            if let Some(id) = self.table_ids.get(table) {
                // We never remove the table entirely, but we need to stop tracking
                // once all observers have been removed.
                self.num_observers[*id] -= 1;
                if self.num_observers[*id] == 0 {
                    requires_version_bump = true;
                }
            }
        }

        if requires_version_bump {
            self.counter = self.counter.saturating_add(1);
        }
    }

    /// Calculate the which tables should be added or removed from a `connection_state` to
    /// make sure it is synced up with the current list.
    ///
    /// This will return the new updated state as well as the list of triggers that should be
    /// created or removed.
    fn calculate_changes(
        &self,
        connection_state: &FixedBitSet,
    ) -> (FixedBitSet, Vec<ObservedTableOp>) {
        let mut result = connection_state.clone();
        result.grow(self.tables.len());
        let mut changes = Vec::with_capacity(self.tables.len());
        let min_index = connection_state.len().min(self.tables.len());
        for i in 0..min_index {
            let is_tracking = connection_state[i];
            let num_observers = self.num_observers[i];

            if is_tracking && num_observers == 0 {
                changes.push(ObservedTableOp::Remove(self.tables[i].clone(), i));
                result.set(i, false);
            } else if !is_tracking && num_observers != 0 {
                changes.push(ObservedTableOp::Add(self.tables[i].clone(), i));
                result.set(i, true);
            }
        }

        // Process any new tables that might be missing.
        for i in min_index..self.num_observers.len() {
            if self.num_observers[i] != 0 {
                changes.push(ObservedTableOp::Add(self.tables[i].clone(), i));
                result.set(i, true);
            }
        }

        (result, changes)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::watcher::{ObservedTables, TableObserver, Watcher};
    use std::collections::BTreeSet;
    use std::sync::atomic::Ordering;

    pub struct TestObserver {
        tables: Vec<String>,
    }

    impl TableObserver for TestObserver {
        fn tables(&self) -> Vec<String> {
            self.tables.clone()
        }
        fn on_tables_changed(&self, _: &BTreeSet<String>) {}
    }

    pub(crate) fn new_test_observer(
        tables: impl IntoIterator<Item = &'static str>,
    ) -> Box<dyn TableObserver + Send + 'static> {
        Box::new(TestObserver {
            tables: Vec::from_iter(tables.into_iter().map(|t| t.to_string())),
        })
    }

    fn check_table_counter(tables: &ObservedTables, name: &str, expected: usize) {
        let idx = *tables
            .table_ids
            .get(name)
            .expect("could not find table by name");
        assert_eq!(tables.num_observers[idx], expected);
    }

    #[test]
    fn test_observer_tables_version_counter() {
        let service = Watcher::new().unwrap();

        let mut version = service.tables_version.load(Ordering::Relaxed);
        let observer_1 = new_test_observer(["foo", "bar"]);
        let observer_2 = new_test_observer(["bar"]);
        let observer_3 = new_test_observer(["bar", "omega"]);

        // Adding new observer triggers change.
        let observer_1_id = service.add_observer(observer_1).unwrap();
        service.with_tables(|tables| {
            assert_eq!(tables.num_observers.len(), 2);
            check_table_counter(tables, "foo", 1);
            check_table_counter(tables, "bar", 1);
        });
        version += 1;
        assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

        // Adding an observer for only bar does not change version counter.
        let observer_2_id = service.add_observer(observer_2).unwrap();
        service.with_tables(|tables| {
            assert_eq!(tables.num_observers.len(), 2);
            check_table_counter(tables, "foo", 1);
            check_table_counter(tables, "bar", 2);
        });
        assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

        // Adding this observer causes another change
        let observer_3_id = service.add_observer(observer_3).unwrap();
        service.with_tables(|tables| {
            assert_eq!(tables.num_observers.len(), 3);
            check_table_counter(tables, "foo", 1);
            check_table_counter(tables, "omega", 1);
            check_table_counter(tables, "bar", 3);
        });
        version += 1;
        assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

        // Remove observer 2 causes no version change.
        service.remove_observer(observer_2_id).unwrap();
        service.with_tables(|tables| {
            assert_eq!(tables.num_observers.len(), 3);
            check_table_counter(tables, "foo", 1);
            check_table_counter(tables, "bar", 2);
            check_table_counter(tables, "omega", 1);
        });
        assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

        // Remove observer 3 causes version change.
        service.remove_observer(observer_3_id).unwrap();
        service.with_tables(|tables| {
            assert_eq!(tables.num_observers.len(), 3);
            check_table_counter(tables, "foo", 1);
            check_table_counter(tables, "bar", 1);
            check_table_counter(tables, "omega", 0);
        });
        version += 1;
        assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

        // Remove observer 1 causes version change.
        service.remove_observer(observer_1_id).unwrap();
        service.with_tables(|tables| {
            assert_eq!(tables.num_observers.len(), 3);
            check_table_counter(tables, "foo", 0);
            check_table_counter(tables, "bar", 0);
            check_table_counter(tables, "omega", 0);
        });
        version += 1;
        assert_eq!(version, service.tables_version.load(Ordering::Relaxed));
    }
}
