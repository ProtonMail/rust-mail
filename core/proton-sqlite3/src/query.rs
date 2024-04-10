use crate::{
    InProcessTrackerService, Observer, SqliteConnection, SqliteConnectionPool, TrackedObserverId,
};
use parking_lot::lock_api::Mutex;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::error;

/// Behavior for queries used by [`Observed`] or [`Live`].
pub trait Observable: Send + Sync + Clone + 'static {
    type Output: Send + 'static + Default;
    /// Debug name for logging.
    fn debug_name(&self) -> &'static str;

    /// List of tables that will trigger the query re-execution.
    fn tables(&self) -> Vec<String>;

    /// Execute the database query.
    ///
    /// # Errors
    /// Should return error if the query fails to execute.
    fn execute(&self, connection: &SqliteConnection) -> rusqlite::Result<Self::Output>;
}

pub trait ObserverCallback<I: Send>: Send + Sync {
    fn on_changed(&self, input: I);
}

impl<I: Send, F: Fn(I) + Send + Sync> ObserverCallback<I> for F {
    fn on_changed(&self, input: I) {
        (self)(input);
    }
}

/// Contains the state required for the execution of [`Observable`]. Drop this type
/// to stop the observation.
/// The observation will start as soon as this type is constructed.
pub struct Observed {
    service: InProcessTrackerService,
    observer_id: TrackedObserverId,
}

impl Drop for Observed {
    fn drop(&mut self) {
        self.service.remove_observer(self.observer_id);
    }
}

impl Observed {
    pub fn new<Q: Observable + 'static>(
        service: InProcessTrackerService,
        query: Q,
        callback: impl ObserverCallback<Q::Output> + 'static,
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

struct QueryTrackerObserver<Q: Observable> {
    tables: Vec<String>,
    query: Q,
    callback: Box<dyn ObserverCallback<Q::Output>>,
}

impl<Q: Observable> Observer for QueryTrackerObserver<Q> {
    fn tables(&self) -> &[String] {
        &self.tables
    }

    fn on_tables_changed(&self, _: &BTreeSet<String>, pool: &SqliteConnectionPool) {
        tracing::debug!("Observable Query {} updated", self.query.debug_name());
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
    update_cb: Option<Box<dyn LiveQueryUpdated>>,
}

impl<Q: Send + Sized> SharedValue<Q> {
    fn new(cb: Option<Box<dyn LiveQueryUpdated>>) -> Self {
        Self {
            has_new_value: AtomicBool::new(false),
            value: Mutex::new(None),
            update_cb: cb,
        }
    }

    fn store(&self, value: Q) {
        {
            let mut guard = self.value.lock();
            *guard = Some(value);
            self.has_new_value.store(true, Ordering::Release);
        }
        if let Some(cb) = &self.update_cb {
            cb.on_live_query_updated();
        }
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
pub trait LiveQueryUpdated: Send + Sync {
    fn on_live_query_updated(&self);
}

/// Builder for [`Live`].
pub struct LiveQueryBuilder {
    initialization_mode: InitializationMode,
    callback: Option<Box<dyn LiveQueryUpdated>>,
    service: InProcessTrackerService,
}

impl LiveQueryBuilder {
    #[must_use]
    pub fn new(service: InProcessTrackerService) -> Self {
        Self {
            initialization_mode: InitializationMode::None,
            callback: None,
            service,
        }
    }

    /// Initialize the first value on the current executing thread.
    #[must_use]
    pub fn with_foreground_initializer(mut self) -> Self {
        self.initialization_mode = InitializationMode::Foreground;
        self
    }

    /// Initialize the first value on a background thread.
    #[must_use]
    pub fn with_background_initializer(mut self) -> Self {
        self.initialization_mode = InitializationMode::Background;
        self
    }

    /// Callback to be called each time a new value is available.
    #[must_use]
    pub fn with_callback(mut self, callback: impl LiveQueryUpdated + 'static) -> Self {
        self.callback = Some(Box::new(callback));
        self
    }

    /// Build the live query.
    #[must_use]
    pub fn build<Q: Observable>(self, query: Q) -> Live<Q> {
        let initializer: &dyn LiveQueryInitializer<Q> = match self.initialization_mode {
            InitializationMode::None => &DefaultLiveQueryInitializer {},
            InitializationMode::Foreground => &ForegroundLiveQueryInitializer {},
            InitializationMode::Background => &BackgroundLiveQueryInitializer {},
        };
        Live::new(self.service, query, self.callback, initializer)
    }
}

/// Automatically keep the output of the given [`Observable`] up to date with the latest value
/// when changes are made to the database.
pub struct Live<Q: Observable> {
    observed_query: Option<Observed>,
    last_value: RefCell<Q::Output>,
    shared: Arc<SharedValue<Q::Output>>,
}

impl<Q: Observable + 'static> Live<Q> {
    /// Create a new instance of live query
    ///
    /// # Params
    ///
    /// * `service`: The tracker service in which the query will register with
    /// * `query`: Query implementation to run
    /// * `cb`: Callback when the query has a new value due to changes made to observed tables
    /// * `initializer`: Initialization mode
    fn new(
        service: InProcessTrackerService,
        query: Q,
        cb: Option<Box<dyn LiveQueryUpdated>>,
        initializer: &dyn LiveQueryInitializer<Q>,
    ) -> Self {
        let shared = Arc::new(SharedValue::new(cb));
        let value = initializer.initialize(&query, service.db_pool(), &shared);
        let shared_cloned = shared.clone();
        let query = Observed::new(service, query, move |new_value| {
            shared_cloned.store(new_value);
        });
        Self {
            last_value: RefCell::new(value),
            observed_query: Some(query),
            shared,
        }
    }

    /// Get the latest value or the last updated value.
    pub fn value(&self) -> impl Deref<Target = Q::Output> + '_ {
        if let Some(new_value) = self.shared.take() {
            {
                *self.last_value.borrow_mut() = new_value;
            }
        }

        self.last_value.borrow()
    }

    /// Terminate the observer for this query and stop receiving updates.
    pub fn disconnect(&mut self) {
        self.observed_query = None;
    }
}

/// Builder for [`SharedLive`].
pub struct SharedLiveQueryBuilder {
    initialization_mode: InitializationMode,
    callback: Option<Box<dyn LiveQueryUpdated>>,
    service: InProcessTrackerService,
}

impl SharedLiveQueryBuilder {
    /// Create a new instance.
    #[must_use]
    pub fn new(service: InProcessTrackerService) -> Self {
        Self {
            initialization_mode: InitializationMode::None,
            callback: None,
            service,
        }
    }

    /// Initialize the first value on the current executing thread.
    #[must_use]
    pub fn with_foreground_initializer(mut self) -> Self {
        self.initialization_mode = InitializationMode::Foreground;
        self
    }

    /// Initialize the first value on a background thread.
    #[must_use]
    pub fn with_background_initializer(mut self) -> Self {
        self.initialization_mode = InitializationMode::Background;
        self
    }

    /// Callback to be called each time a new value is available.
    #[must_use]
    pub fn with_callback(mut self, callback: impl LiveQueryUpdated + 'static) -> Self {
        self.callback = Some(Box::new(callback));
        self
    }

    /// Build the query type.
    #[must_use]
    pub fn build<Q: Observable>(self, query: Q) -> SharedLive<Q> {
        let initializer: &dyn LiveQueryInitializer<Q> = match self.initialization_mode {
            InitializationMode::None => &DefaultLiveQueryInitializer {},
            InitializationMode::Foreground => &ForegroundLiveQueryInitializer {},
            InitializationMode::Background => &BackgroundLiveQueryInitializer {},
        };
        SharedLive::new(self.service, query, self.callback, initializer)
    }
}

/// Same as [`Live`], but can be accessed from multiple threads.
pub struct SharedLive<Q: Observable> {
    observed_query: parking_lot::Mutex<Option<Observed>>,
    last_value: parking_lot::Mutex<Q::Output>,
    shared: Arc<SharedValue<Q::Output>>,
}

impl<Q: Observable + 'static> SharedLive<Q> {
    /// Create a new instance of shared live query
    ///
    /// # Params
    ///
    /// * `service`: The tracker service in which the query will register with
    /// * `query`: Query implementation to run
    /// * `cb`: Callback when the query has a new value due to changes made to observed tables
    /// * `initializer`: Initialization mode
    fn new(
        service: InProcessTrackerService,
        query: Q,
        cb: Option<Box<dyn LiveQueryUpdated>>,
        initializer: &dyn LiveQueryInitializer<Q>,
    ) -> Self {
        let shared = Arc::new(SharedValue::new(cb));
        let value = initializer.initialize(&query, service.db_pool(), &shared);
        let shared_cloned = shared.clone();
        let query = Observed::new(service, query, move |new_value| {
            shared_cloned.store(new_value);
        });
        Self {
            last_value: parking_lot::Mutex::new(value),
            observed_query: parking_lot::Mutex::new(Some(query)),
            shared,
        }
    }

    /// Get the latest value or the last updated value.
    pub fn value(&self) -> impl Deref<Target = Q::Output> + '_ {
        let mut accessor = self.last_value.lock();
        if let Some(new_value) = self.shared.take() {
            *accessor = new_value;
        }
        accessor
    }

    /// Terminate the observer for this query and stop receiving updates.
    pub fn disconnect(&self) {
        *self.observed_query.lock() = None;
    }
}

fn run_query<Q: Observable>(query: &Q, pool: &SqliteConnectionPool) -> rusqlite::Result<Q::Output> {
    let conn = pool.acquire()?;
    query.execute(&conn)
}

trait LiveQueryInitializer<Q: Observable>: 'static + Send + Sync {
    fn initialize(
        &self,
        query: &Q,
        pool: &SqliteConnectionPool,
        shared_value: &Arc<SharedValue<Q::Output>>,
    ) -> Q::Output;
}

struct DefaultLiveQueryInitializer {}

impl<Q: Observable> LiveQueryInitializer<Q> for DefaultLiveQueryInitializer {
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

impl<Q: Observable> LiveQueryInitializer<Q> for BackgroundLiveQueryInitializer {
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
impl<Q: Observable> LiveQueryInitializer<Q> for ForegroundLiveQueryInitializer {
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
