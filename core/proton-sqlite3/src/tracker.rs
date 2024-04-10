use crate::{SqliteConnection, SqliteConnectionPool};
use fixedbitset::FixedBitSet;
use parking_lot::RwLock;
use rusqlite::Transaction;
use slotmap::{new_key_type, SlotMap};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use tracing::{error, Level};

/// Observer for changes made in the database.
#[cfg_attr(test, mockall::automock)]
pub trait Observer: Send + Sync {
    fn tables(&self) -> &[String];
    fn on_tables_changed(&self, tables: &BTreeSet<String>, pool: &SqliteConnectionPool);
}

new_key_type! {pub struct TrackedObserverId;}

/// Synchronizes with [`TrackingService`] to keep track of changes made to tables of interest.
pub struct TrackingConnection {
    connection: SqliteConnection,
    tracker: LocalTracker,
}

impl AsRef<SqliteConnection> for TrackingConnection {
    fn as_ref(&self) -> &SqliteConnection {
        &self.connection
    }
}

impl AsMut<SqliteConnection> for TrackingConnection {
    fn as_mut(&mut self) -> &mut SqliteConnection {
        &mut self.connection
    }
}

impl TrackingConnection {
    /// Create a new tracking connection with a given service.
    ///
    /// # Params
    /// * `conn`: Database connection.
    /// * `service`: Instance where to publish changes.
    ///
    /// # Errors
    /// Returns error if we can not initialize the tracking tables.
    pub fn new(
        mut conn: SqliteConnection,
        service: InProcessTrackerService,
    ) -> rusqlite::Result<Self> {
        let tracker = LocalTracker::new(service, &mut conn)?;

        Ok(Self {
            connection: conn,
            tracker,
        })
    }

    /// Transactions need to be created through this helper function so that they work correctly.
    ///
    /// # Errors
    /// Returns error if the transaction failed to submit or if there was an issue with tracking
    /// changes.
    pub fn tx<E: From<rusqlite::Error>, T, F: FnMut(&mut Transaction) -> Result<T, E>>(
        &mut self,
        closure: F,
    ) -> Result<T, E> {
        self.tracker.sync(&mut self.connection)?;
        let r = self.connection.tx(closure)?;
        self.tracker.check_for_changes(&mut self.connection)?;
        Ok(r)
    }
}

/// Provides a notification service when database table change. To be notified of changes, register
/// an observer with [`InProcessTrackerService::add_observer`]. This service will work with multiple
/// [`TrackingConnection`] as long as it happens in the same OS process.
#[derive(Clone)]
pub struct InProcessTrackerService {
    inner: Arc<TrackerServiceInner>,
    sender: Sender<TrackerResult>,
    pool: SqliteConnectionPool,
}

impl InProcessTrackerService {
    /// Create a new instance of an in process tracker service.
    ///
    /// # Errors
    /// Returns error if the worker thread fails to spawn.
    pub fn new(pool: SqliteConnectionPool) -> std::io::Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let inner = Arc::new(TrackerServiceInner::new());
        let inner_cloned = inner.clone();
        let pool_cloned = pool.clone();
        std::thread::Builder::new()
            .name("db_tracker".into())
            .spawn(move || {
                TrackerServiceInner::background_loop(receiver, inner_cloned, pool_cloned);
            })?;
        Ok(Self {
            inner,
            sender,
            pool,
        })
    }

    /// Register a new observer with a list of interested tables. This function returns an
    /// [`TrackedObserverId`] which can later be used to remove the current observer;
    #[must_use]
    pub fn add_observer(&self, observer: Box<dyn Observer>) -> TrackedObserverId {
        self.inner.add_observer(observer)
    }

    /// Create a new tracking connection.
    ///
    /// # Errors
    /// Returns error if we could not acquire a database connection.
    pub fn new_connection(&self) -> rusqlite::Result<TrackingConnection> {
        let conn = self.pool.acquire()?;
        TrackingConnection::new(conn, self.clone())
    }

    /// Remove an observer.
    pub fn remove_observer(&self, id: TrackedObserverId) {
        self.inner.remove_observer(id);
    }

    /// Get the underlying database connection pool.
    #[must_use]
    pub fn db_pool(&self) -> &SqliteConnectionPool {
        &self.pool
    }

    fn publish_changes(&self, result: TrackerResult) {
        if self.sender.send(result).is_err() {
            error!("Tracking service could not communicate with background thread");
        }
    }
}

#[derive(Debug)]
struct TrackerResult {
    table_ids: FixedBitSet,
}

struct LocalTrackerState {
    tracked_tables: FixedBitSet,
    last_sync_version: u64,
}

impl LocalTrackerState {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            tracked_tables: FixedBitSet::with_capacity(capacity),
            last_sync_version: 0,
        }
    }
}

struct LocalTracker {
    service: InProcessTrackerService,
    state: LocalTrackerState,
}

impl LocalTrackerState {
    fn should_sync(&self, service: &TrackerServiceInner) -> Option<u64> {
        let service_version = service.tables_version.load(Ordering::Acquire);
        if service_version > self.last_sync_version {
            Some(service_version)
        } else {
            None
        }
    }

    fn calculate_sync_changes(
        &self,
        service: &TrackerServiceInner,
    ) -> Option<(FixedBitSet, Vec<ObservedTableOp>)> {
        let (new_tracker_state, tracker_changes) = {
            let accessor = service.tables.read();
            accessor.calculate_changes(&self.tracked_tables)
        };

        if tracker_changes.is_empty() {
            return None;
        }

        Some((new_tracker_state, tracker_changes))
    }

    fn commit_sync_changes(&mut self, new_tracker_state: FixedBitSet, new_version: u64) {
        // Update local tracker bitset
        self.tracked_tables = new_tracker_state;
        self.last_sync_version = new_version;
    }

    fn sync<E, F: FnOnce(&[ObservedTableOp]) -> Result<(), E>>(
        &mut self,
        service: &TrackerServiceInner,
        apply_fn: F,
    ) -> Result<(), E> {
        let Some(new_version) = self.should_sync(service) else {
            return Ok(());
        };

        tracing::trace!("Syncing tables from observer");
        let Some((new_tracker_state, tracker_changes)) = self.calculate_sync_changes(service)
        else {
            tracing::trace!("No changes");
            return Ok(());
        };
        (apply_fn)(&tracker_changes)?;
        self.commit_sync_changes(new_tracker_state, new_version);
        Ok(())
    }
}

const TRACKER_TABLE_NAME: &str = "proton_sqlite_tracker";
impl LocalTracker {
    fn new(
        service: InProcessTrackerService,
        connection: &mut SqliteConnection,
    ) -> rusqlite::Result<Self> {
        Self::init(connection)?;
        Ok(Self {
            service,
            state: LocalTrackerState::with_capacity(8),
        })
    }

    fn init(connection: &mut SqliteConnection) -> rusqlite::Result<()> {
        // create tracking table and cleanup previous data if re-used from a connection pool.
        connection.tx(|tx| {
            tx.execute(&format!("CREATE TEMP TABLE IF NOT EXISTS {TRACKER_TABLE_NAME} (table_id INTEGER PRIMARY KEY, updated INTEGER)"),())?;
            tx.execute(&format!("DELETE FROM {TRACKER_TABLE_NAME}"),())
        })?;

        Ok(())
    }

    #[tracing::instrument(level=Level::TRACE, skip(self, connection))]
    fn sync(&mut self, connection: &mut SqliteConnection) -> rusqlite::Result<()> {
        self.state.sync(&self.service.inner, |tracker_changes| {
            connection.tx(|tx| -> rusqlite::Result<()> {
                for change in tracker_changes {
                    match change {
                        ObservedTableOp::Add(table_name, id) => {
                            tracing::trace!("Add watcher for table {table_name} id={id}");
                            Self::create_triggers(tx, table_name, *id)?;
                        }
                        ObservedTableOp::Remove(table_name, id) => {
                            tracing::trace!("Remove watcher for table {table_name}");
                            Self::drop_triggers(tx, table_name, *id)?;
                        }
                    }
                }
                Ok(())
            })
        })?;

        Ok(())
    }

    #[tracing::instrument(level=Level::TRACE, skip(self, connection))]
    fn check_for_changes(&mut self, connection: &mut SqliteConnection) -> rusqlite::Result<()> {
        let changes = self.check_tables(connection)?;
        self.service.publish_changes(changes);
        Ok(())
    }
    fn check_tables(
        &mut self,
        connection: &mut SqliteConnection,
    ) -> rusqlite::Result<TrackerResult> {
        let query = format!("SELECT table_id  FROM {TRACKER_TABLE_NAME} WHERE updated=1");
        let mut modified_tables = FixedBitSet::with_capacity(self.state.tracked_tables.len());

        {
            let mut stmt = connection.prepare(&query)?;
            for row in stmt.query_map((), |r| r.get(0))? {
                let id = row?;
                tracing::trace!("Table {} has been modified", id);
                modified_tables.set(id, true);
            }
        }

        if !modified_tables.is_clear() {
            // Reset updated values.
            connection.tx(|tx| -> rusqlite::Result<usize> {
                tx.execute(
                    &format!("UPDATE {TRACKER_TABLE_NAME} SET updated=0 WHERE updated=1"),
                    (),
                )
            })?;
        }

        Ok(TrackerResult {
            table_ids: modified_tables,
        })
    }

    fn create_triggers(tx: &mut Transaction, table: &str, id: usize) -> rusqlite::Result<()> {
        use std::fmt::Write;
        let mut query = String::with_capacity(64);
        for (trigger, name) in TRIGGER_LIST {
            query.clear();
            write!(
                &mut query,
                r#"
CREATE TEMP TRIGGER IF NOT EXISTS trigger_{table}_{name} AFTER {trigger} ON {table}
BEGIN
    UPDATE  {TRACKER_TABLE_NAME} SET updated=1 WHERE table_id={id};
END
            "#
            )
            .expect("should not fail");
            tx.execute(&query, ())?;
        }

        query.clear();
        write!(&mut query, "INSERT INTO {TRACKER_TABLE_NAME} VALUES (?,0)")
            .expect("Should not fail");
        tx.execute(&query, [id])?;
        Ok(())
    }

    fn drop_triggers(tx: &mut Transaction, table: &str, id: usize) -> rusqlite::Result<()> {
        use std::fmt::Write;
        let mut query = String::with_capacity(64);
        for (_, name) in TRIGGER_LIST {
            query.clear();
            write!(query, "DROP TRIGGER IF EXISTS trigger_{table}_{name}")
                .expect("should not fail");
            tx.execute(&query, ())?;
        }
        query.clear();
        write!(
            &mut query,
            "DELETE FROM {TRACKER_TABLE_NAME} WHERE table_id=?"
        )
        .expect("Should not fail");
        tx.execute(&query, [id])?;
        Ok(())
    }
}

const TRIGGER_LIST: [(&str, &str); 3] = [
    ("INSERT", "insert"),
    ("UPDATE", "update"),
    ("DELETE", "delete"),
];

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ObservedTableOp {
    Add(String, usize),
    Remove(String, usize),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct ObservedTableId(usize);

/// Keeps track of all the observed tables. Each table is assigned an unique value (index) which
/// is then propagated to all the trackers.
struct ObservedTables {
    table_ids: BTreeMap<String, ObservedTableId>,
    tables: Vec<String>,
    num_observers: Vec<usize>,
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

    fn track_tables(&mut self, tables: impl Iterator<Item = String>) {
        for table in tables {
            match self.table_ids.entry(table.clone()) {
                Entry::Vacant(v) => {
                    let id = ObservedTableId(self.num_observers.len());
                    self.tables.push(table.clone());
                    self.num_observers.push(1);
                    v.insert(id);
                    self.counter += 1;
                }
                Entry::Occupied(o) => {
                    let id = o.get().0;
                    let current = self.num_observers[id];
                    if current == 0 {
                        // We should start following this table again. If it is not
                        // 0, we are already observing it.
                        self.counter += 1;
                    }
                    self.num_observers[o.get().0] = current + 1;
                }
            }
        }
    }

    fn untrack_tables<'i>(&mut self, tables: impl Iterator<Item = &'i String>) {
        for table in tables {
            if let Some(id) = self.table_ids.get(table) {
                // We never remove the table entirely, but we need to stop tracking
                // once all observers have been removed.
                self.num_observers[id.0] -= 1;
                if self.num_observers[id.0] == 0 {
                    self.counter += 1;
                }
            }
        }
    }

    fn calculate_changes(&self, tracker: &FixedBitSet) -> (FixedBitSet, Vec<ObservedTableOp>) {
        let mut result = tracker.clone();
        result.grow(self.tables.len());
        let mut changes = Vec::with_capacity(self.tables.len());
        let min_index = tracker.len().min(self.tables.len());
        for i in 0..min_index {
            let is_tracking = tracker[i];
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

struct TrackerServiceInner {
    tables: RwLock<ObservedTables>,
    observers: RwLock<SlotMap<TrackedObserverId, ObserverWrapper>>,
    tables_version: AtomicU64,
}

impl TrackerServiceInner {
    fn new() -> Self {
        Self {
            tables: RwLock::new(ObservedTables::new()),
            observers: RwLock::new(SlotMap::with_capacity_and_key(8)),
            tables_version: AtomicU64::new(0),
        }
    }

    pub fn add_observer(&self, observer: Box<dyn Observer>) -> TrackedObserverId {
        let observer = ObserverWrapper::new(observer);

        self.with_tables_mut(|tables| {
            tables.track_tables(observer.tables_set.iter().cloned());
        });

        {
            let mut accessor = self.observers.write();
            accessor.insert(observer)
        }
    }

    #[cfg(test)]
    fn get_table_id(&self, table: &str) -> Option<ObservedTableId> {
        self.with_tables(|tables| tables.table_ids.get(table).cloned())
    }

    pub fn remove_observer(&self, tracked_observer_id: TrackedObserverId) {
        let Some(observer) = ({
            let mut accessor = self.observers.write();
            accessor.remove(tracked_observer_id)
        }) else {
            return;
        };

        self.with_tables_mut(|tables| tables.untrack_tables(observer.tables_set.iter()));
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
}

struct ObserverWrapper {
    observer: Box<dyn Observer>,
    tables_set: BTreeSet<String>,
}

impl ObserverWrapper {
    fn new(observer: Box<dyn Observer>) -> Self {
        Self {
            tables_set: observer.tables().iter().cloned().collect(),
            observer,
        }
    }
    fn on_table_changes(&self, changed: &BTreeSet<String>, pool: &SqliteConnectionPool) {
        // If at least one of the tables changed, trigger callback;
        if self.tables_set.intersection(changed).next().is_some() {
            self.observer.on_tables_changed(changed, pool);
        }
    }
}

struct TrackedResultRecorder {
    table_ids: FixedBitSet,
    tables: BTreeSet<String>,
}

impl TrackedResultRecorder {
    fn new() -> Self {
        Self {
            tables: BTreeSet::new(),
            table_ids: FixedBitSet::with_capacity(8),
        }
    }

    fn clear(&mut self) {
        self.table_ids.clear();
        self.tables.clear();
    }

    fn merge(&mut self, result: TrackerResult) {
        self.table_ids |= result.table_ids;
    }

    fn has_changes(&self) -> bool {
        !self.table_ids.is_clear()
    }

    fn resolve_table_names(&mut self, service: &TrackerServiceInner) {
        service.with_tables(|observer_tables| {
            for idx in self.table_ids.ones() {
                // Safeguard against some invalid index, just in case.
                if let Some(name) = observer_tables.tables.get(idx).cloned() {
                    self.tables.insert(name);
                }
            }
        });
    }
}

impl TrackerServiceInner {
    #[tracing::instrument(name="TrackerService", level= Level::TRACE, skip(receiver, service, pool))]
    fn background_loop(
        receiver: Receiver<TrackerResult>,
        service: Arc<TrackerServiceInner>,
        pool: SqliteConnectionPool,
    ) {
        let mut recorder = TrackedResultRecorder::new();
        loop {
            recorder.clear();
            match receiver.recv() {
                Ok(result) => {
                    recorder.merge(result);
                    // Try to see if there are any more pending changes queued and ready
                    loop {
                        match receiver.try_recv() {
                            Ok(result) => {
                                recorder.merge(result);
                                // Try to see if there are any more pending changes queued and ready
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
                }
                Err(_) => {
                    return;
                }
            };

            if !recorder.has_changes() {
                continue;
            }

            // resolve tree names;
            recorder.resolve_table_names(&service);

            tracing::trace!("Changes detected on tables: {:?}", recorder.tables);
            // publish changes;
            {
                let accessor = service.observers.read();
                for (_, observer) in accessor.iter() {
                    observer.on_table_changes(&recorder.tables, &pool);
                }
            }
        }
    }
}

#[cfg(test)]
pub struct TestObserver {
    tables: Vec<String>,
}

#[cfg(test)]
impl Observer for TestObserver {
    fn tables(&self) -> &[String] {
        &self.tables
    }
    fn on_tables_changed(&self, _: &BTreeSet<String>, _: &SqliteConnectionPool) {}
}

#[cfg(test)]
fn new_test_observer(
    tables: impl IntoIterator<Item = &'static str>,
) -> Box<dyn Observer + Send + 'static> {
    Box::new(TestObserver {
        tables: Vec::from_iter(tables.into_iter().map(|t| t.to_string())),
    })
}

#[cfg(test)]
fn check_table_counter(tables: &ObservedTables, name: &str, expected: usize) {
    let idx = tables
        .table_ids
        .get(name)
        .expect("could not find table by name")
        .0;
    assert_eq!(tables.num_observers[idx], expected);
}

#[test]
fn test_observer_tables_version_counter() {
    let service = TrackerServiceInner::new();

    let mut version = service.tables_version.load(Ordering::Relaxed);
    let observer_1 = new_test_observer(["foo", "bar"]);
    let observer_2 = new_test_observer(["bar"]);
    let observer_3 = new_test_observer(["bar", "omega"]);

    // Adding new observer triggers change.
    let observer_1_id = service.add_observer(observer_1);
    service.with_tables(|tables| {
        assert_eq!(tables.num_observers.len(), 2);
        check_table_counter(tables, "foo", 1);
        check_table_counter(tables, "bar", 1);
    });
    version += 1;
    assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

    // Adding an observer for only bar does not change version counter.
    let observer_2_id = service.add_observer(observer_2);
    service.with_tables(|tables| {
        assert_eq!(tables.num_observers.len(), 2);
        check_table_counter(tables, "foo", 1);
        check_table_counter(tables, "bar", 2);
    });
    assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

    // Adding this observer causes another change
    let observer_3_id = service.add_observer(observer_3);
    service.with_tables(|tables| {
        assert_eq!(tables.num_observers.len(), 3);
        check_table_counter(tables, "foo", 1);
        check_table_counter(tables, "omega", 1);
        check_table_counter(tables, "bar", 3);
    });
    version += 1;
    assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

    // Remove observer 2 causes no version change.
    service.remove_observer(observer_2_id);
    service.with_tables(|tables| {
        assert_eq!(tables.num_observers.len(), 3);
        check_table_counter(tables, "foo", 1);
        check_table_counter(tables, "bar", 2);
        check_table_counter(tables, "omega", 1);
    });
    assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

    // Remove observer 3 causes version change.
    service.remove_observer(observer_3_id);
    service.with_tables(|tables| {
        assert_eq!(tables.num_observers.len(), 3);
        check_table_counter(tables, "foo", 1);
        check_table_counter(tables, "bar", 1);
        check_table_counter(tables, "omega", 0);
    });
    version += 1;
    assert_eq!(version, service.tables_version.load(Ordering::Relaxed));

    // Remove observer 1 causes version change.
    service.remove_observer(observer_1_id);
    service.with_tables(|tables| {
        assert_eq!(tables.num_observers.len(), 3);
        check_table_counter(tables, "foo", 0);
        check_table_counter(tables, "bar", 0);
        check_table_counter(tables, "omega", 0);
    });
    version += 1;
    assert_eq!(version, service.tables_version.load(Ordering::Relaxed));
}

#[test]
fn test_local_tracker_state() {
    let service = TrackerServiceInner::new();

    let observer_1 = new_test_observer(["foo", "bar"]);
    let observer_2 = new_test_observer(["bar"]);
    let observer_3 = new_test_observer(["bar", "omega"]);

    let mut local_state = LocalTrackerState::with_capacity(4);

    assert!(local_state.should_sync(&service).is_none());
    let observer_id_1 = service.add_observer(observer_1);
    let foo_table_id = service.get_table_id("foo").unwrap().0;
    let bar_table_id = service.get_table_id("bar").unwrap().0;
    {
        let new_version = local_state
            .should_sync(&service)
            .expect("Should have new version");
        let (tracker, ops) = local_state
            .calculate_sync_changes(&service)
            .expect("must have changes");
        assert!(tracker[foo_table_id]);
        assert!(tracker[bar_table_id]);
        assert_eq!(ops.len(), 2);
        assert_eq!(
            ops[0],
            ObservedTableOp::Add("bar".to_string(), bar_table_id)
        );
        assert_eq!(
            ops[1],
            ObservedTableOp::Add("foo".to_string(), foo_table_id)
        );

        local_state.commit_sync_changes(tracker, new_version);
    }

    let observer_id_2 = service.add_observer(observer_2);
    assert!(local_state.should_sync(&service).is_none());

    let observer_id_3 = service.add_observer(observer_3);
    let omega_table_id = service.get_table_id("omega").unwrap().0;
    {
        let new_version = local_state
            .should_sync(&service)
            .expect("Should have new version");
        let (tracker, ops) = local_state
            .calculate_sync_changes(&service)
            .expect("must have changes");
        assert!(tracker[foo_table_id]);
        assert!(tracker[bar_table_id]);
        assert!(tracker[omega_table_id]);
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            ObservedTableOp::Add("omega".to_string(), omega_table_id)
        );

        local_state.commit_sync_changes(tracker, new_version);
    }

    service.remove_observer(observer_id_2);
    assert!(local_state.should_sync(&service).is_none());

    service.remove_observer(observer_id_3);
    {
        let new_version = local_state
            .should_sync(&service)
            .expect("Should have new version");
        let (tracker, ops) = local_state
            .calculate_sync_changes(&service)
            .expect("must have changes");
        assert!(tracker[foo_table_id]);
        assert!(tracker[bar_table_id]);
        assert!(!tracker[omega_table_id]);
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0],
            ObservedTableOp::Remove("omega".to_string(), omega_table_id)
        );

        local_state.commit_sync_changes(tracker, new_version);
    }

    service.remove_observer(observer_id_1);
    {
        let new_version = local_state
            .should_sync(&service)
            .expect("Should have new version");
        let (tracker, ops) = local_state
            .calculate_sync_changes(&service)
            .expect("must have changes");
        assert!(!tracker[foo_table_id]);
        assert!(!tracker[bar_table_id]);
        assert!(!tracker[omega_table_id]);
        assert_eq!(ops.len(), 2);
        assert_eq!(
            ops[0],
            ObservedTableOp::Remove("bar".to_string(), bar_table_id)
        );
        assert_eq!(
            ops[1],
            ObservedTableOp::Remove("foo".to_string(), foo_table_id)
        );

        local_state.commit_sync_changes(tracker, new_version);
    }
}

#[test]
fn test_tracker_result_recorder() {
    let mut recorder = TrackedResultRecorder::new();
    let mut change_1 = TrackerResult {
        table_ids: FixedBitSet::with_capacity(8),
    };
    let mut change_2 = TrackerResult {
        table_ids: FixedBitSet::with_capacity(8),
    };
    change_1.table_ids.set(1, true);
    change_1.table_ids.set(4, true);
    change_2.table_ids.set(3, true);
    change_2.table_ids.set(4, true);

    recorder.merge(change_1);
    recorder.merge(change_2);
    assert!(recorder.has_changes());
    assert!(recorder.table_ids[1]);
    assert!(recorder.table_ids[3]);
    assert!(recorder.table_ids[4]);
}

#[test]
fn test_service() {
    let orig = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig(panic_info);
        std::process::exit(-1);
    }));
    let pool = SqliteConnectionPool::new(crate::SqliteMode::InMemory, false);
    let tracker_service =
        InProcessTrackerService::new(pool.clone()).expect("failed to create tracker service");

    {
        let mut conn = pool.acquire().expect("failed to acquire connection");
        conn.tx(|tx| {
            tx.execute(
                "CREATE TABLE foo (id INTEGER PRIMARY KEY AUTOINCREMENT, v INTEGER)",
                (),
            )?;
            tx.execute("CREATE TABLE bar (v INTEGER UNIQUE)", ())
        })
        .unwrap();
    }

    let tracked_tables = vec!["foo".to_string(), "bar".to_string()];

    let mut observer = MockObserver::new();
    observer.expect_tables().return_const(tracked_tables);

    let mut sequence = mockall::Sequence::new();

    let foo_table_set = BTreeSet::from_iter(["foo".to_string()]);
    let bar_table_set = BTreeSet::from_iter(["bar".to_string()]);
    let foo_bar_table_set = BTreeSet::from_iter(["foo".to_string(), "bar".to_string()]);

    // Synchronization to avoid merging of changes;
    let (sender, receiver) = std::sync::mpsc::sync_channel::<()>(0);

    let cloned_sender = sender.clone();

    use mockall::predicate;

    observer
        .expect_on_tables_changed()
        .with(predicate::eq(foo_table_set), predicate::always())
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |_, _| {
            cloned_sender.send(()).unwrap();
        });
    let cloned_sender = sender.clone();
    observer
        .expect_on_tables_changed()
        .with(predicate::eq(bar_table_set.clone()), predicate::always())
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |_, _| {
            cloned_sender.send(()).unwrap();
        });
    let cloned_sender = sender.clone();
    observer
        .expect_on_tables_changed()
        .with(predicate::eq(bar_table_set), predicate::always())
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |_, _| {
            cloned_sender.send(()).unwrap();
        });
    let cloned_sender = sender.clone();
    observer
        .expect_on_tables_changed()
        .with(predicate::eq(foo_bar_table_set), predicate::always())
        .times(1)
        .in_sequence(&mut sequence)
        .returning(move |_, _| {
            cloned_sender.send(()).unwrap();
        });

    let _ = tracker_service.add_observer(Box::new(observer));

    let mut conn = TrackingConnection::new(pool.acquire().unwrap(), tracker_service.clone())
        .expect("Failed to init tracking pool");

    conn.tx(|tx| tx.execute("INSERT INTO foo VALUES( null,10)", ()))
        .unwrap();
    receiver.recv().unwrap();
    conn.tx(|tx| tx.execute("INSERT OR REPLACE INTO bar VALUES(10)", ()))
        .unwrap();
    receiver.recv().unwrap();
    conn.tx(|tx| tx.execute("INSERT OR REPLACE INTO bar VALUES(10)", ()))
        .unwrap();
    receiver.recv().unwrap();
    conn.tx(|tx| {
        tx.execute("DELETE FROM foo WHERE v=10", ())?;
        tx.execute("DELETE FROM bar WHERE v=10", ())
    })
    .unwrap();
    receiver.recv().unwrap();
}
