use crate::{
    InProcessTrackerService, SqliteConnection, SqliteConnectionPool, TrackedObserverId,
    TrackerObserver,
};
use parking_lot::lock_api::Mutex;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::error;

/// Behavior for queries used by [`ObservedQuery`] or [`LiveQuery`].
pub trait ObservableQuery: Send + Sync + Clone + 'static {
    type Output: Send + 'static + Default;
    /// Debug name for logging.
    fn debug_name(&self) -> &'static str;

    /// List of tables that will trigger the query re-execution.
    fn tables(&self) -> Vec<String>;

    /// Execute the database query.
    fn execute(&self, connection: &SqliteConnection) -> rusqlite::Result<Self::Output>;
}

pub trait QueryObserverCallback<I: Send>: Send + Sync {
    fn on_changed(&self, input: I);
}

impl<I: Send, F: Fn(I) + Send + Sync> QueryObserverCallback<I> for F {
    fn on_changed(&self, input: I) {
        (self)(input);
    }
}

/// Contains the state required for the execution of [`ObservableQuery`]. Drop this type
/// to stop the observation.
/// The observation will start as soon as this type is constructed.
pub struct ObservedQuery {
    service: InProcessTrackerService,
    observer_id: TrackedObserverId,
}

impl Drop for ObservedQuery {
    fn drop(&mut self) {
        self.service.remove_observer(self.observer_id)
    }
}

impl ObservedQuery {
    pub fn new<Q: ObservableQuery + 'static>(
        service: InProcessTrackerService,
        query: Q,
        callback: impl QueryObserverCallback<Q::Output> + 'static,
    ) -> Self {
        let observer = Box::new(QueryTrackerObserver {
            tables: query.tables(),
            query,
            callback: Box::new(callback),
        });
        let observer_id = service.add_observer(observer);
        Self {
            service,
            observer_id,
        }
    }
}

struct QueryTrackerObserver<Q: ObservableQuery> {
    tables: Vec<String>,
    query: Q,
    callback: Box<dyn QueryObserverCallback<Q::Output>>,
}

impl<Q: ObservableQuery> TrackerObserver for QueryTrackerObserver<Q> {
    fn tables(&self) -> &[String] {
        &self.tables
    }

    fn on_tables_changed(&self, _: &BTreeSet<String>, pool: &SqliteConnectionPool) {
        let r = match run_query(&self.query, pool) {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Query({}) failed to execute: {}",
                    self.query.debug_name(),
                    e
                );
                return;
            }
        };
        self.callback.on_changed(r);
    }
}

struct SharedValue<Q: Send + Sized> {
    has_new_value: AtomicBool,
    value: parking_lot::Mutex<Option<Q>>,
}

impl<Q: Send + Sized> SharedValue<Q> {
    fn new() -> Self {
        Self {
            has_new_value: AtomicBool::new(false),
            value: Mutex::new(None),
        }
    }

    fn store(&self, value: Q) {
        let mut guard = self.value.lock();
        *guard = Some(value);
        self.has_new_value.store(true, Ordering::Release);
    }

    fn take(&self) -> Option<Q> {
        if let Ok(true) =
            self.has_new_value
                .compare_exchange(true, false, Ordering::Acquire, Ordering::Relaxed)
        {
            return self.value.lock().take();
        }

        None
    }
}

/// Called every time the value on the live query has changed.
pub trait LiveQueryUpdated {
    fn on_live_query_updated(&self);
}

/// Builder for [`LiveQuery`].
pub struct LiveQueryBuilder {
    initialization_mode: InitializationMode,
    callback: Option<Box<dyn LiveQueryUpdated>>,
    service: InProcessTrackerService,
}

impl LiveQueryBuilder {
    pub fn new(service: InProcessTrackerService) -> Self {
        Self {
            initialization_mode: InitializationMode::None,
            callback: None,
            service,
        }
    }

    /// Initialize the first value on the current executing thread.
    pub fn with_foreground_initializer(mut self) -> Self {
        self.initialization_mode = InitializationMode::Foreground;
        self
    }

    /// Initialize the first value on a background thread.
    pub fn with_background_initializer(mut self) -> Self {
        self.initialization_mode = InitializationMode::Background;
        self
    }

    /// Callback to be called each time a new value is available.
    pub fn with_callback(mut self, callback: impl LiveQueryUpdated + 'static) -> Self {
        self.callback = Some(Box::new(callback));
        self
    }

    pub fn build<Q: ObservableQuery>(self, query: Q) -> LiveQuery<Q> {
        let initializer: &dyn LiveQueryInitializer<Q> = match self.initialization_mode {
            InitializationMode::None => &DefaultLiveQueryInitializer {},
            InitializationMode::Foreground => &ForegroundLiveQueryInitializer {},
            InitializationMode::Background => &BackgroundLiveQueryInitializer {},
        };
        LiveQuery::new(self.service, query, self.callback, initializer)
    }
}

/// Automatically keep the output of the given [`ObservableQuery`] up to date with the latest value
/// when changes are made to the database.
pub struct LiveQuery<Q: ObservableQuery> {
    _query: ObservedQuery,
    last_value: RefCell<Q::Output>,
    shared: Arc<SharedValue<Q::Output>>,
    update_cb: Option<Box<dyn LiveQueryUpdated>>,
}

impl<Q: ObservableQuery + 'static> LiveQuery<Q> {
    fn new(
        service: InProcessTrackerService,
        query: Q,
        cb: Option<Box<dyn LiveQueryUpdated>>,
        initializer: &dyn LiveQueryInitializer<Q>,
    ) -> Self {
        let shared = Arc::new(SharedValue::new());
        let value = initializer.initialize(&query, service.db_pool(), &shared);
        let shared_cloned = shared.clone();
        let query = ObservedQuery::new(service, query, move |new_value| {
            shared_cloned.store(new_value);
        });
        Self {
            last_value: RefCell::new(value),
            _query: query,
            shared,
            update_cb: cb,
        }
    }

    /// Get the latest value or the last updated value.
    pub fn value(&self) -> impl Deref<Target = Q::Output> + '_ {
        if let Some(new_value) = self.shared.take() {
            *self.last_value.borrow_mut() = new_value;
            if let Some(cb) = &self.update_cb {
                cb.on_live_query_updated();
            }
        }

        self.last_value.borrow()
    }
}

fn run_query<Q: ObservableQuery>(
    query: &Q,
    pool: &SqliteConnectionPool,
) -> rusqlite::Result<Q::Output> {
    let conn = pool.acquire()?;
    query.execute(&conn)
}

trait LiveQueryInitializer<Q: ObservableQuery>: 'static + Send + Sync {
    fn initialize(
        &self,
        query: &Q,
        pool: &SqliteConnectionPool,
        shared_value: &Arc<SharedValue<Q::Output>>,
    ) -> Q::Output;
}

struct DefaultLiveQueryInitializer {}

impl<Q: ObservableQuery> LiveQueryInitializer<Q> for DefaultLiveQueryInitializer {
    fn initialize(
        &self,
        _: &Q,
        _: &SqliteConnectionPool,
        _: &Arc<SharedValue<Q::Output>>,
    ) -> Q::Output {
        Q::Output::default()
    }
}

struct BackgroundLiveQueryInitializer {}

impl<Q: ObservableQuery> LiveQueryInitializer<Q> for BackgroundLiveQueryInitializer {
    fn initialize(
        &self,
        query: &Q,
        pool: &SqliteConnectionPool,
        shared_value: &Arc<SharedValue<Q::Output>>,
    ) -> Q::Output {
        let query = query.clone();
        let pool = pool.clone();
        let shared_value = shared_value.clone();
        std::thread::spawn(move || match run_query(&query, &pool) {
            Ok(v) => shared_value.store(v),
            Err(e) => {
                error!(
                    "Query ({}) failed to run during initialization: {e}",
                    query.debug_name()
                );
            }
        });

        Default::default()
    }
}

struct ForegroundLiveQueryInitializer {}
impl<Q: ObservableQuery> LiveQueryInitializer<Q> for ForegroundLiveQueryInitializer {
    fn initialize(
        &self,
        query: &Q,
        pool: &SqliteConnectionPool,
        _: &Arc<SharedValue<Q::Output>>,
    ) -> Q::Output {
        match run_query(query, pool) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    "Query ({}) failed to run during initialization: {e}",
                    query.debug_name()
                );
                Default::default()
            }
        }
    }
}

enum InitializationMode {
    None,
    Foreground,
    Background,
}
