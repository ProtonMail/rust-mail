//! Paginator for managing large result sets with live updates.
//!
//! The [`Paginator`] struct provides a powerful and efficient way to manage
//! large result sets while supporting live updates. It is designed to work
//! seamlessly with the existing [`Model`] trait and [`Stash`] database
//! interface, offering intuitive pagination control.
//!
//! # Key features
//!
//!   1. **Sliding window**: Maintains a sliding window of pages, pre-fetching
//!      the next page in the background and saving to the database. The offset
//!      position of the window is adaptive, depending on updates to the result
//!      set, i.e. additions to the start of the result set will maintain the
//!      current perceived position by incrementing the cursor position and
//!      moving the window forward.
//!
//!   2. **Live updates**: Supports real-time updates to the result set,
//!      handling insertions, updates, and deletions.
//!
//!   3. **Cursor management**: Keeps track of the current position in the
//!      result set, ensuring consistent navigation even as the underlying data
//!      changes.
//!
//!   4. **Asynchronous operations**: All database operations are asynchronous,
//!      preventing blocking.
//!
//! # How it works
//!
//! ## Initialisation
//!
//! When a [`Paginator`] is created, it:
//!
//!   1. Fetches the first page of results from the database, using known data,
//!      which returns immediately.
//!   2. Queries the API in the background, to fetch the next page.
//!   3. Sets up a listener for live updates.
//!
//! ## Navigation
//!
//! As the client navigates through the result set:
//!
//!   - **Moving to the next page**
//!
//!       1. If the next page is available, it's fetched from the database.
//!       2. The sliding window is updated, and the cursor is moved forward.
//!       3. The next page is pre-fetched from the API in the background.
//!
//! ## Live updates
//!
//! The paginator listens for changes to the result set:
//!
//!   1. When a change occurs, it's processed immediately.
//!   2. The total count and cursor are updated as necessary.
//!   3. The client is alerted via a channel.
//!
//! ### Handling specific changes
//!
//!   - **Insertion**
//!   
//!       - If the new record belongs before the cursor, the cursor is moved
//!         forward.
//!       - The results count is incremented.
//!   
//!   - **Update**
//!   
//!       - No effect on the cursor or paginator, but the client will be
//!         notified.
//!   
//!   - **Deletion**
//!   
//!       - If the deleted record was before the cursor, the cursor is moved
//!         backward.
//!       - The results count is decremented.
//!
//! ## Cursor management
//!
//! The cursor represents the starting position of the current page in the
//! overall result set. It's adjusted as the client navigates and as live
//! updates occur, ensuring that the client's view of the data remains
//! consistent even as the underlying data changes.
//!
//! The concept of a "page" is nominal, as the approach does not actually use
//! true pages, even through the word is used for convenience. Instead, it
//! maintains a sliding window of results, with the cursor indicating the start
//! of the current frame. This behaves the same as pages for a static result
//! set, but adapts to changes in the result set which is where the behaviour
//! differs.
//!
//! # Usage for clients
//!
//! Clients interact with the [`Paginator`] through a set of intuitive methods:
//!
//!   1. [`next_page()`](Paginator::next_page()):
//!      Move to and get the next page. This will be obtained from the database,
//!      and the page after will be fetched from the API into the database in
//!      the background. The next page is always calculated from the current
//!      cursor position.
//!
//!   2. [`result_count()`](Paginator::result_count()):
//!      Get the total number of results. This can change over time.
//!

use core::error::Error;
use core::future::Future;
use core::num::NonZeroUsize;
use flume::Sender as QueueSender;
use indoc::formatdoc;
use proton_sqlite3::rusqlite::hooks::Action;
use stash::exports::{SqliteError, ToSql, ToSqlOutput, Value};
use stash::orm::{perform_find, Model, ResultsetChange};
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::spawn;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, warn};

#[cfg(test)]
#[path = "tests/paginator/paginator.rs"]
mod tests_paginator;

#[cfg(test)]
#[path = "tests/paginator/data_source.rs"]
mod tests_data_source;

/// Represents a parameter for a query.
#[derive(Clone, Debug)]
#[allow(clippy::exhaustive_enums)]
pub enum Param {
    /// A null value.
    Null,

    /// An integer value.
    Integer(i64),

    /// A floating-point value.
    Real(f64),

    /// A text value.
    Text(String),

    /// A binary value.
    Blob(Vec<u8>),
}

impl ToSql for Param {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(match *self {
            Self::Null => ToSqlOutput::Owned(Value::Null),
            Self::Integer(ref i) => (*i).into(),
            Self::Real(ref f) => (*f).into(),
            Self::Text(ref s) => s.as_str().into(),
            Self::Blob(ref b) => ToSqlOutput::Owned(Value::Blob(b.clone())),
        })
    }
}

/// Defines a remote source of data that feeds a [`Paginator`] when new
/// pages need to be fetched from some location.
pub trait DataSource: Send + Sync {
    /// The item fetched from the source.
    type Item: Model;
    /// Error that may occur when interacting with the remote source.
    type Error: Error + Send + 'static + From<StashError>;

    /// Total number of elements that are available.
    ///
    /// # Params
    ///
    /// * `stash` - Database connection.
    fn total(&self, stash: &Stash) -> impl Future<Output = Result<usize, Self::Error>> + Send;

    /// Sync the first page of this source.
    ///
    /// This function is only called during initialization if no elements
    /// are present in the database.
    ///
    /// # Params
    /// * `page_size` - Number of elements per page.
    /// * `stash` - Database connection.
    fn sync_first_page(
        &self,
        page_size: NonZeroUsize,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send;

    /// Sync the page after a given element.
    ///
    /// This function is called every time the cursor advances to next
    /// page. This function also passes in a copy of all the items in
    /// the current page in order to satisfy paginator sources which are
    /// not paginated via a simple index.
    ///
    /// # Params
    ///
    /// * `cursor_index` : Current position of the last synced element
    /// * `page_size`    : Number of elements per page.
    /// * `element`      : Last synced element. `None` if no element was ever synced.
    /// * `stash` - Database connection.
    fn sync_page_after(
        &self,
        cursor_index: usize,
        page_size: NonZeroUsize,
        element: Option<&Self::Item>,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send;

    /// Save `records` to the database.
    ///
    /// This method is here to allow the data source to override the default
    /// save method. Some cases which require overriding `Model::Save`
    /// will not work correctly otherwise.
    ///
    /// # Errors
    ///
    /// Return error if the db operation failed.
    fn save_to_database(
        &self,
        mut records: Vec<Self::Item>,
        stash: &Stash,
    ) -> impl Future<Output = Result<Vec<Self::Item>, StashError>> {
        let stash = stash.clone();
        async move {
            let tx = stash.transaction().await?;
            for record in &mut records {
                record.save_using(&tx).await?;
            }
            tx.commit().await?;
            Ok(records)
        }
    }
}

/// Represents a paginated view of a result set.
///
/// The [`Paginator`] manages the result set, providing pagination capabilities
/// and handling live updates. It can be used for both paginated and
/// non-paginated result sets, offering a consistent interface for data access.
///
/// It manages a sliding window of results, pre-fetching adjacent pages for
/// quick access while maintaining a consistent view of the data even as it
/// changes. It handles live updates, cursor management, and provides an
/// intuitive navigation experience through the result set.
///
#[derive(Debug)]
pub struct Paginator<T: Model, R: DataSource<Item = T> + 'static> {
    /// Shared state between the [`Paginator`] and the background watcher.
    shared: Arc<Mutex<Shared>>,

    /// The number of records per page. Assuming no changes to the result set,
    /// this will remain constant. However, if the data changes, the number for
    /// a particular page may vary.
    page_size: NonZeroUsize,

    /// The parameters used in the query.
    params: Vec<Param>,

    /// The query logic used for finding records. This will be repeated when
    /// obtaining additional pages from the database.
    query_logic: String,

    /// The [`Stash`] instance used for database operations. This is not used
    /// for the initial query (that uses whatever was supplied), but is required
    /// for subsequent queries and for live updates.
    stash: Stash,

    /// The [`DataSource`] from where pagination data will be synced
    /// from.
    remote: Option<Arc<R>>,

    #[allow(clippy::type_complexity)] // Can't define an alias due to trait bounds
    pre_fetch_task: Mutex<Option<JoinHandle<Result<Vec<T>, R::Error>>>>,
}

/// Shared state between the [`Paginator`] and the background watcher.
#[derive(Debug)]
struct Shared {
    /// The current cursor position in the result set. This indicates the start
    /// of the current frame.
    cursor_index: usize,
    /// The total number of records in the result set. This will be kept updated
    /// as changes occur to the result set.
    cursor_row_id: Option<u64>,
    last_synced_index: usize,
    last_synced_row_id: Option<u64>,
    /// The total number of records in the result set. This will be kept updated
    /// as changes occur to the result set.
    row_count: usize,
    /// Recently synced record ids to filter out unnecessary create record events.
    /// If the user is watching for change in this table they can skip these
    /// as these events are a direct result of the user's action.
    recently_synced: HashSet<u64>,
}

impl<T: Model, R: DataSource<Item = T>> Paginator<T, R> {
    /// Creates a new [`Paginator`] instance.
    ///
    /// This method is typically called through the [`Model::find()`] method and
    /// similar find-based functionality.
    ///
    /// Note that all results will always be supplied via a [`Paginator`], even
    /// if pagination is not active.
    ///
    /// # Parameters
    ///
    /// * `query_logic` - The query logic to use for finding the records. This
    ///                   should be a string that represents the conditions and
    ///                   ordering for the query, as may be required, but NOT
    ///                   offset and limit. It can be empty. Note that each part
    ///                   of the logic is optional — so if conditions are
    ///                   passed, for instance, the `WHERE` keyword needs to be
    ///                   included.
    /// * `params`      - The parameters to use in the query. These should be in
    ///                   the order they are expected in the query logic, and
    ///                   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                   to use for finding the records. Note that this will
    ///                   only be respected for the initial query, and not for
    ///                   any subsequent queries that are performed as a result
    ///                   of updates to the result set when pagination is active
    ///                   — those will use the underlying [`Stash`] instance.
    /// * `page_size`   - The number of records per page. Note that pages are
    ///                   adaptive windows onto the result set, and so the
    ///                   actual number of records returned may vary from this
    ///                   value if the result set changes. The page size must
    ///                   not be zero.
    /// * `remote`      - Implementation of [`DataSource`] where pagination
    ///                   data will be synchronized from.
    /// * `queue`       - An optional queue to send changes to. If this is
    ///                   provided, the function will listen for changes to the
    ///                   result set and send them to the queue. This is useful
    ///                   for live updates.
    ///
    /// # Errors
    ///
    /// See [`Model::find()`].
    ///
    pub async fn new<Q, A>(
        query_logic: Q,
        params: Vec<Param>,
        interface: &A,
        page_size: NonZeroUsize,
        remote: Option<R>,
        queue: Option<QueueSender<ResultsetChange<T, T::IdType>>>,
    ) -> Result<Self, R::Error>
    where
        Q: Into<String> + Send,
        A: Into<AgnosticInterface> + Interface,
        R: DataSource,
    {
        let paginator = Self {
            shared: Arc::new(Mutex::new(Shared {
                cursor_index: 0,
                cursor_row_id: None,
                row_count: 0,
                recently_synced: HashSet::new(),
                last_synced_index: 0,
                last_synced_row_id: None,
            })),
            page_size,
            params,
            query_logic: query_logic.into(),
            stash: interface.stash().clone(),
            remote: remote.map(Arc::new),
            pre_fetch_task: Mutex::new(None),
        };

        paginator.initialize(interface).await?;

        paginator.start_update_listener(queue);

        Ok(paginator)
    }

    /// Initializes the paginator by fetching the initial set of records.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records. Note that this will only be
    ///                 respected for the initial query, and not for any
    ///                 subsequent queries that are performed as a result of
    ///                 updates to the result set when pagination is active —
    ///                 those will use the underlying [`Stash`] instance.
    ///
    #[allow(clippy::cast_possible_truncation)]
    async fn initialize<A>(&self, interface: &A) -> Result<(), R::Error>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        // Acquire lock to prevent interference from watcher thread.
        // Note: watcher is currently initialized after this function,
        // but at least we are safe if this changes in the future.
        let mut shared = self.shared.lock().await;

        let initial_records = interface
            .query_values::<_, u64>(
                format!(
                    "SELECT rowid AS value FROM {} {} LIMIT {}",
                    T::table_name(),
                    self.query_logic,
                    self.page_size,
                ),
                convert_params(&self.params),
            )
            .await?;

        if let Some(remote) = self.remote.as_ref() {
            let total = remote.total(&self.stash).await?;
            shared.row_count = total.max(initial_records.len());
            let remote_cloned = Arc::clone(remote);
            let page_size = self.page_size;
            let stash = self.stash.clone();
            *self.pre_fetch_task.lock().await = Some(spawn(async move {
                remote_cloned.sync_first_page(page_size, &stash).await
            }));
        } else {
            shared.row_count = initial_records.len();
        }

        shared.cursor_row_id = initial_records.first().copied();

        Ok(())
    }

    /// Starts the update listener to handle live updates.
    fn start_update_listener(&self, sender: Option<QueueSender<ResultsetChange<T, T::IdType>>>) {
        let stash = self.stash.clone();
        let query_logic = self.query_logic.clone();
        let params = self.params.clone();
        let shared_cloned = Arc::clone(&self.shared);
        let remote_cloned = self.remote.clone();

        drop(spawn(async move {
            let changed_query = formatdoc!(
                "
                    SELECT
                        {}.rowid AS rowid, *
                    FROM
                        {}
                    WHERE
                        rowid = ?
                    LIMIT
                        1
                ",
                T::table_name(),
                T::table_name(),
            );
            // For now this is blanket subscriber — this will be optimised later to
            // only listen for changes that are relevant to the current query.
            if let Ok(receiver) = stash.subscribe().await {
                loop {
                    match receiver.recv_async().await {
                        Ok(notification) => {
                            if notification.table != T::table_name() {
                                continue;
                            }

                            let mut shared = shared_cloned.lock().await;

                            // Update initial synced records

                            match notification.action {
                                Action::SQLITE_DELETE => {
                                    // Always handle delete, but we still need to remove
                                    // the element.
                                    shared.recently_synced.remove(&notification.row);
                                }
                                Action::SQLITE_INSERT | Action::SQLITE_UPDATE => {
                                    // If a record is inserted and matches a recently synced id
                                    // we can ignore this notification.
                                    // We also apply the same for update as its possible to have
                                    // tables which perform create or update.
                                    if shared.recently_synced.remove(&notification.row) {
                                        continue;
                                    }
                                }
                                _ => {
                                    warn!("Unknown action");
                                }
                            }

                            if let Some(change) = T::handle_notification(
                                notification,
                                // We don't use this in the same way here
                                &mut HashMap::new(),
                                &stash,
                                &changed_query,
                            )
                            .await
                            {
                                if let Err(e) = Self::handle_change(
                                    &change,
                                    &query_logic,
                                    params.clone(),
                                    &mut shared,
                                    remote_cloned.as_ref(),
                                    &stash,
                                    sender.as_ref(),
                                )
                                .await
                                {
                                    error!("Error handling change: {:?}", e);
                                }
                            }
                        }
                        Err(error) => {
                            // In theory this should never happen, but we also can't do anything with it
                            error!("Lost connection to change feed: {error}");
                            break;
                        }
                    }
                }
            }
        }));
    }

    /// Handles a change in the result set.
    ///
    /// This accepts references to shared state elements due to the listening
    /// loop not being able to operate on self.
    ///
    /// # Parameters
    ///
    /// * `change`        - The change that occurred in the result set.
    /// * `query_logic`   - The query logic used for finding records.
    /// * `params`        - The parameters used in the query.
    /// * `row_count`     - The total number of records in the result set.
    /// * `cursor_index`  - The current cursor position in the result set.
    /// * `cursor_row_id` - The current row ID of the record at the cursor.
    /// * `stash`         - The [`Stash`] instance used for database operations.
    /// * `sender`        - The sender for live updates.
    ///
    #[allow(clippy::too_many_arguments)]
    async fn handle_change(
        change: &ResultsetChange<T, T::IdType>,
        query_logic: &str,
        params: Vec<Param>,
        shared: &mut Shared,
        remote: Option<&Arc<R>>,
        stash: &Stash,
        sender: Option<&flume::Sender<ResultsetChange<T, T::IdType>>>,
    ) -> Result<(), StashError> {
        match *change {
            ResultsetChange::Inserted(_) | ResultsetChange::Deleted(_) => {
                Self::update_cursor_and_row_id(
                    &mut shared.cursor_index,
                    &mut shared.cursor_row_id,
                    change,
                    query_logic,
                    &params,
                    stash,
                )
                .await?;
                Self::update_cursor_and_row_id(
                    &mut shared.last_synced_index,
                    &mut shared.last_synced_row_id,
                    change,
                    query_logic,
                    &params,
                    stash,
                )
                .await?;
                // Manually update total row count based on event.
                let mut total_update_fallback = || {
                    if let ResultsetChange::Inserted(_) = *change {
                        shared.row_count = shared.row_count.saturating_add(1);
                    } else if let ResultsetChange::Deleted(_) = *change {
                        shared.row_count = shared.row_count.saturating_sub(1);
                    }
                };

                if let Some(remote) = remote {
                    match remote.total(stash).await {
                        Ok(v) => {
                            shared.row_count = v;
                        }
                        Err(_) => {
                            total_update_fallback();
                        }
                    }
                } else {
                    total_update_fallback();
                }
            }
            ResultsetChange::Updated(_) => {
                // No change to cursor or count for updates
            }
            _ => {
                error!("Pattern not covered");
            }
        }

        // Notify the client of the change if they have subscribed.
        if let Some(sender) = sender {
            sender
                .send(change.clone())
                .map_err(|_err| StashError::Custom("Failed to send update".into()))?;
        }

        Ok(())
    }

    /// Update a cursor index and respective row id based on ongoing changes.
    async fn update_cursor_and_row_id(
        cursor_index: &mut usize,
        cursor_row_id: &mut Option<u64>,
        change: &ResultsetChange<T, T::IdType>,
        query_logic: &str,
        params: &[Param],
        stash: &Stash,
    ) -> Result<(), StashError> {
        // Re-run the query to check if the cursor position needs to change. This
        // gets the first record at the offset of the cursor, and if doesn't have
        // the same ID as the current cursor record, we need to adjust the cursor.
        let cursor_record: Option<T> = perform_find(
            #[allow(clippy::unwrap_used)]
            &paging_query(query_logic, *cursor_index, NonZeroUsize::new(1).unwrap()),
            convert_params(params),
            &stash.clone().into(),
            None,
        )
        .await?
        .into_iter()
        .next();

        match cursor_record {
            Some(record) => {
                if let Some(cursor_row_id) = *cursor_row_id {
                    #[allow(clippy::cast_lossless, clippy::unwrap_used)]
                    if cursor_row_id != record.row_id().unwrap() {
                        // The change was made before the cursor position
                        if let ResultsetChange::Inserted(_) = *change {
                            *cursor_index = cursor_index.saturating_add(1);
                        } else if let ResultsetChange::Deleted(_) = *change {
                            *cursor_index = cursor_index.saturating_sub(1);
                        }
                    }
                }
                // Update cursor
                *cursor_row_id = record.row_id();
            }
            None => {
                // We've reached the end of the result set, meaning a deletion before the
                // cursor position
                if let ResultsetChange::Deleted(_) = *change {
                    // try to find something a valid element for the cursor.
                    loop {
                        *cursor_index = cursor_index.saturating_sub(1);
                        if let Some(cursor_record) = perform_find::<_, T>(
                            #[allow(clippy::unwrap_used)]
                            &paging_query(
                                query_logic,
                                *cursor_index,
                                NonZeroUsize::new(1).unwrap(),
                            ),
                            convert_params(params),
                            &stash.clone().into(),
                            None,
                        )
                        .await?
                        .into_iter()
                        .next()
                        {
                            *cursor_row_id = Some(cursor_record.row_id().unwrap());
                            break;
                        }

                        // if we reach this point and we still don't
                        // have an element then there is nothing left.
                        if *cursor_index == 0 {
                            *cursor_row_id = None;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Retrieves a collection of values.
    ///
    /// # Params
    ///
    /// * `cursor_index` - Index of the cursor to load from.
    /// * `count`        - Number of elements to load after `cursor_index`
    ///
    /// # Errors
    ///
    /// Returns an error if the current page could not be fetched from the
    /// database.
    ///
    async fn values_at(
        &self,
        cursor_index: usize,
        count: NonZeroUsize,
    ) -> Result<Vec<T>, StashError> {
        T::find(
            paging_query(&self.query_logic, cursor_index, count),
            convert_params(&self.params),
            &self.stash,
            None,
        )
        .await
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// If there are no values available at this time for the next page this
    /// method will return an empty collection.
    ///
    /// This method will pre-fetch the next page in the background and will
    /// wait until the previous pre-fetching of a page has finished before
    /// returning to the user.
    ///
    /// The first time this method is called, it will wait on the pre-fetch
    /// scheduled by the [`initialize()`].
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the database.
    ///
    #[allow(clippy::missing_panics_doc)]
    pub async fn next_page(&self) -> Result<Vec<T>, R::Error> {
        if let Some(remote) = self.remote.as_ref() {
            let mut pre_fetch_task = self.pre_fetch_task.lock().await;
            if let Some(task) = pre_fetch_task.take() {
                let new_records = task.await.map_err(|_| {
                    StashError::Custom("Failed to join pre fetch task".to_owned())
                })??;

                let mut shared = self.shared.lock().await;
                if new_records.is_empty() {
                    let last_synced_element = self
                        .values_at(shared.last_synced_index, NonZeroUsize::new(1).unwrap())
                        .await?;
                    let remote_cloned = Arc::clone(remote);
                    let page_size = self.page_size;
                    let last_synced_index = shared.last_synced_index;
                    let stash = self.stash.clone();
                    *pre_fetch_task = Some(spawn(async move {
                        let last_element = last_synced_element.first();
                        remote_cloned
                            .sync_page_after(last_synced_index, page_size, last_element, &stash)
                            .await
                    }));
                    Ok(vec![])
                } else {
                    let new_records = remote.save_to_database(new_records, &self.stash).await?;

                    for element in &new_records {
                        shared.recently_synced.insert(element.row_id().unwrap());
                    }

                    let current_page = self
                        .values_at(
                            shared.last_synced_index,
                            NonZeroUsize::new(new_records.len()).unwrap(),
                        )
                        .await?;

                    shared.cursor_index = shared.last_synced_index.saturating_add(1);
                    shared.last_synced_index = shared
                        .cursor_index
                        .saturating_add(current_page.len())
                        .saturating_sub(1);
                    shared.cursor_row_id = current_page.first().map(|v| v.row_id().unwrap());
                    shared.last_synced_row_id = current_page.last().map(|v| v.row_id().unwrap());

                    // Schedule new sync
                    let remote_cloned = Arc::clone(remote);
                    let last_element = current_page.last().cloned();
                    let last_element_cursor = shared.last_synced_index;
                    let page_size = self.page_size;
                    let stash = self.stash.clone();
                    *pre_fetch_task = Some(spawn(async move {
                        remote_cloned
                            .sync_page_after(
                                last_element_cursor,
                                page_size,
                                last_element.as_ref(),
                                &stash,
                            )
                            .await
                    }));
                    Ok(current_page)
                }
            } else {
                Ok(vec![])
            }
        } else {
            // fallback for lack of remote source
            let mut shared = self.shared.lock().await;

            let current_page = self
                .values_at(shared.last_synced_index, self.page_size)
                .await?;

            if !current_page.is_empty() {
                shared.cursor_index = shared.last_synced_index.saturating_add(1);
                shared.last_synced_index = shared
                    .cursor_index
                    .saturating_add(current_page.len())
                    .saturating_sub(1);
                shared.cursor_row_id = current_page.first().map(|v| v.row_id().unwrap());
                shared.last_synced_row_id = current_page.last().map(|v| v.row_id().unwrap());
            }

            Ok(current_page)
        }
    }

    /// Reloads all data up to the cursor.
    ///
    /// Grabs **ALL** the rows that have been seen so far, without any kind of
    /// limit or pagination, from the start right up to the current cursor
    /// position.
    ///
    /// This does not attempt to prefetch anything, and does not update any
    /// pagination state data.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be fetched from the database.
    ///
    #[allow(clippy::missing_panics_doc)]
    pub async fn reload(&self) -> Result<Vec<T>, StashError> {
        let range = {
            let shared = self.shared.lock().await;
            shared.last_synced_index
        };
        match NonZeroUsize::new(range) {
            None => Ok(vec![]),
            Some(range) => self.values_at(0, range).await,
        }
    }

    /// Retrieves the total number of records in the result set.
    pub async fn result_count(&self) -> usize {
        self.shared.lock().await.row_count
    }
}

/// Constructs a query for paging through a result set.
///
/// # Parameters
///
/// * `query_logic`  - The query logic to use for finding the records.
/// * `cursor_index` - The current cursor position in the result set.
/// * `page_size`    - The number of records per page.
///
fn paging_query(query_logic: &str, cursor_index: usize, page_size: NonZeroUsize) -> String {
    formatdoc!(
        "
            {}
            LIMIT
                {}
            OFFSET
                {}
        ",
        query_logic,
        page_size,
        cursor_index,
    )
}

/// Converts a slice of [`Param`] instances into a vector of boxed [`ToSql`]
/// instances.
///
/// # Parameters
///
/// * `params` - The slice of parameters to convert.
///
fn convert_params(params: &[Param]) -> Vec<Box<dyn ToSql + Send>> {
    #[allow(trivial_casts)]
    params
        .iter()
        .map(|p| Box::new(p.clone()) as Box<dyn ToSql + Send>)
        .collect()
}
