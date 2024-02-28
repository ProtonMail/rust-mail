#[derive(Debug, thiserror::Error)]
pub enum SqliteWatcherError {
    #[error("The watcher is only available when using file based storage")]
    InvalidMode,
    #[error("The watcher has been closed")]
    WatcherClosed,
    #[error("Notify Error: {0}")]
    Notify(#[from] notify::Error),
    #[error("Sqlite Error: {0}")]
    SQL(#[from] rusqlite::Error),
}

/// When watching a database, [`Self::on_db_update`] will be called.
pub trait SqliteWatcherHandler: Send + 'static {
    fn on_db_update(&mut self, v: Result<(), SqliteWatcherError>);
}

impl<T: FnMut(Result<(), SqliteWatcherError>) + Send + 'static> SqliteWatcherHandler for T {
    fn on_db_update(&mut self, v: Result<(), SqliteWatcherError>) {
        (self)(v)
    }
}

/// Observe a database from a [`SqliteConnectionPool`] for changes.
pub struct SqliteWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl SqliteWatcher {
    pub(super) fn new(watcher: notify::RecommendedWatcher) -> Self {
        Self { _watcher: watcher }
    }
}
