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
//!   - **Moving to the previous page**
//!
//!       1. The previous page is fetched from the database.
//!       2. The sliding window is updated, and the cursor is moved backward.
//!       3. No API interaction is performed, as the previous page is expected
//!          to already be in the database.
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
//!   1. [`current_page()`](Paginator::current_page()):
//!      Get the current page. This will be obtained from the database, and is
//!      always based on the current cursor position.
//!
//!   2. [`next_page()`](Paginator::next_page()):
//!      Move to and get the next page. This will be obtained from the database,
//!      and the page after will be fetched from the API into the database in
//!      the background. The next page is always calculated from the current
//!      cursor position.
//!
//!   3. [`previous_page()`](Paginator::previous_page()):
//!      Move to and get the previous page. This will be obtained from the
//!      database. The previous page is always calculated from the current
//!      cursor position.
//!
//!   4. [`first_page()`](Paginator::first_page()):
//!      Get the first page. This will be obtained from the database. Moving to
//!      the first page will reset the cursor position to the start of the
//!      result set.
//!
//!   5. [`current_page_number()`](Paginator::current_page_number()):
//!      Get the current page number. Note that this is somewhat arbitrary, as
//!      the concept of pages is nominal, and so the page number calculated for
//!      the current cursor position can change as the result set changes. The
//!      sliding window approach fits better with an infinite scroll model,
//!      where there are no page numbers.
//!
//!   6. [`page_count()`](Paginator::page_count()):
//!      Get the total number of pages. This can change over time.
//!
//!   7. [`result_count()`](Paginator::result_count()):
//!      Get the total number of results. This can change over time.
//!
//!   8. [`has_next_page()`](Paginator::has_next_page()):
//!      Check if there's a next page available.
//!
//!   9. [`has_previous_page()`](Paginator::has_previous_page()):
//!      Check if there's a previous page available.
//!

use core::error::Error;
use core::future::Future;
use core::num::NonZeroU32;
use indoc::formatdoc;
use sqlite_watcher::watcher::TableObserver;
use stash::exports::{SqliteError, ToSql, ToSqlOutput, Value};
use stash::orm::Model;
use stash::stash::{Stash, StashError, Tether, WatcherHandle};
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::Mutex;

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
    fn total(&self, tether: &Tether) -> impl Future<Output = Result<usize, Self::Error>> + Send;

    /// Sync the first page of this source.
    ///
    /// This function is only called during initialization if no elements
    /// are present in the database.
    ///
    /// # Params
    /// * `page_size` - Number of elements per page.
    /// * `stash`     - Database connection.
    fn sync_first_page(
        &self,
        page_size: NonZeroU32,
        tether: &mut Tether,
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
    /// * `cursor_index` : Current position of the cursor in the new page.
    /// * `page_size`    : Number of elements per page.
    /// * `elements`     : Elements that are in the current page.
    ///
    /// * `stash` - Database connection.
    fn sync_page_after(
        &self,
        cursor_index: u32,
        page_size: NonZeroU32,
        elements: Vec<Self::Item>,
        tether: &mut Tether,
    ) -> impl Future<Output = Result<Vec<Self::Item>, Self::Error>> + Send;

    fn watch_tables(&self) -> Vec<String> {
        vec![Self::Item::table_name().to_string()]
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
    page_size: NonZeroU32,

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
    remote: Arc<R>,
}

pub struct PaginatorWatcher<T: Model, R: DataSource<Item = T> + 'static> {
    sender: flume::Sender<()>,
    remote: Arc<R>,
}

impl<T: Model, R: DataSource<Item = T>> TableObserver for PaginatorWatcher<T, R> {
    fn tables(&self) -> Vec<String> {
        self.remote.watch_tables()
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for PaginatorWatcher: {}", e);
            })
            .ok();
    }
}

/// Shared state between the [`Paginator`] and the background watcher.
#[derive(Debug)]
struct Shared {
    /// The current cursor position in the result set. This indicates the start
    /// of the current frame.
    cursor_index: u32,
    /// The previous cursor position in the result set.
    last_cursor_index: u32,
    /// The total number of records in the result set. This will be kept updated
    /// as changes occur to the result set.
    cursor_row_id: Option<u64>,
    /// The total number of records in the result set. This will be kept updated
    /// as changes occur to the result set.
    row_count: u32,
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
    /// * `local_first` - Load local data immediately, to return to the caller
    ///                   without the delay of remote lookup. If set to `false`,
    ///                   no results will be returned until the remote API calls
    ///                   have completed. This only affects the first call to
    ///                   the paginator.
    /// * `queue`       - An optional queue to send changes to. If this is
    ///                   provided, the function will listen for changes to the
    ///                   result set and send them to the queue. This is useful
    ///                   for live updates.
    ///
    /// # Errors
    ///
    /// See [`Model::find()`].
    ///
    pub async fn new<Q>(
        query_logic: Q,
        params: Vec<Param>,
        stash: &Stash,
        page_size: NonZeroU32,
        remote: R,
        local_first: bool,
    ) -> Result<Self, R::Error>
    where
        Q: Into<String> + Send,
        R: DataSource,
    {
        let paginator = Self {
            shared: Arc::new(Mutex::new(Shared {
                cursor_index: 0,
                last_cursor_index: 0,
                cursor_row_id: None,
                row_count: 0,
            })),
            page_size,
            params,
            query_logic: query_logic.into(),
            stash: stash.clone(),
            remote: Arc::new(remote),
        };

        paginator.initialize(local_first).await?;

        Ok(paginator)
    }

    pub fn watch(&self) -> Result<WatcherHandle, StashError> {
        self.stash.subscribe_to(|sender| {
            Box::new(PaginatorWatcher {
                sender,
                remote: self.remote.clone(),
            })
        })
    }

    /// Initializes the paginator by fetching the initial set of records.
    ///
    /// # Parameters
    ///
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                   to use for finding the records. Note that this will
    ///                   only be respected for the initial query, and not for
    ///                   any subsequent queries that are performed as a result
    ///                   of updates to the result set when pagination is active
    ///                   — those will use the underlying [`Stash`] instance.
    /// * `local_first` - Load local data immediately, to return to the caller
    ///                   without the delay of remote lookup. If set to `false`,
    ///                   no results will be returned until the remote API calls
    ///                   have completed. This only affects the first call to
    ///                   the paginator. **NOTE:** At present the background API
    ///                   query has been disabled and so the behaviour is as if
    ///                   `local_first` is always `true`.
    ///
    #[allow(clippy::cast_possible_truncation)]
    async fn initialize(&self, _local_first: bool) -> Result<(), R::Error> {
        let mut tether = self.stash.connection();
        let total = self.remote.total(&tether).await?;

        let mut initial_records = T::find(
            format!("{} LIMIT {}", self.query_logic, self.page_size,),
            convert_params(&self.params),
            &tether,
        )
        .await?;

        // Acquire lock to prevent interference from watcher thread.
        // Note: watcher is currently initialized after this function,
        // but at least we are safe if this changes in the future.
        let mut shared = self.shared.lock().await;

        if initial_records.is_empty()
            || (initial_records.len() < self.page_size.get() as usize
                && initial_records.len() < total)
        {
            initial_records = self
                .remote
                .sync_first_page(self.page_size, &mut tether)
                .await?;
        }

        shared.row_count = total.max(initial_records.len()) as u32;
        shared.cursor_row_id = initial_records.first().map(|v| v.row_id().unwrap());

        Ok(())
    }

    /// Handles a change in the result set.
    ///
    /// This accepts references to shared state elements due to the listening
    /// loop not being able to operate on self.
    ///
    #[allow(clippy::single_match_else)]
    async fn handle_change(&self) -> Result<Vec<T>, R::Error> {
        let mut shared = self.shared.lock().await;
        let tether = self.stash.connection();
        let mut current_items = T::find(
            paging_query(
                &self.query_logic,
                0,
                NonZeroU32::new(shared.cursor_index.saturating_add(self.page_size.into())).unwrap(),
            ),
            convert_params(&self.params),
            &tether,
        )
        .await?;

        let cursor_record = current_items
            .iter()
            .enumerate()
            .find(|(_idx, record)| record.row_id() == shared.cursor_row_id);
        match cursor_record {
            Some((idx, _record)) => {
                if shared.last_cursor_index < shared.cursor_index {
                    // Items moved
                    shared.last_cursor_index = shared.cursor_index;
                    let mut next_page = self.next_page_(&mut shared).await?;
                    next_page.pop(); // do not leak new items to the page
                    current_items.extend(next_page);
                } else {
                    // Set previous cursor index to reference if the items have moved
                    shared.last_cursor_index = u32::try_from(idx).unwrap_or_default();
                }
            }
            None => {
                let next_index = shared.cursor_index.saturating_add(self.page_size.get());
                let next_page = self.page_at(next_index, &tether).await?;
                let cursor_record = next_page
                    .iter()
                    .enumerate()
                    .find(|(_idx, record)| record.row_id() == shared.cursor_row_id);
                match cursor_record {
                    Some((idx, _record)) => {
                        // Cursor record found in next page
                        shared.cursor_index = u32::try_from(idx).unwrap_or_default();
                        shared.last_cursor_index = shared.cursor_index;
                        current_items.extend(next_page);
                    }
                    None => {
                        // Cursor record not found in next page
                        shared.cursor_index = u32::try_from(current_items.len())
                            .unwrap_or_default()
                            .saturating_sub(self.page_size.into());
                        shared.last_cursor_index = shared.cursor_index;
                        shared.cursor_row_id = current_items
                            .get(shared.cursor_index.saturating_sub(1) as usize)
                            .and_then(Model::row_id);
                    }
                }
            }
        }

        match self.remote.total(&tether).await {
            Ok(v) => {
                if let Ok(v) = v.try_into() {
                    shared.row_count = v;
                }
            }
            Err(_) => {
                tracing::error!("Failed to get total count");
            }
        }

        Ok(current_items)
    }

    /// Retrieves the results of the current page.
    ///
    /// # Errors
    ///
    /// Returns an error if the current page could not be fetched from the
    /// database.
    ///
    pub async fn current_page(&self) -> Result<Vec<T>, StashError> {
        let current_index = { self.shared.lock().await.cursor_index };
        let tether = self.stash.connection();
        self.page_at(current_index, &tether).await
    }

    /// Retrieves the page at the given cursor index.
    ///
    /// # Params
    ///
    /// * `cursor_index` - Index of the cursor to load from.
    /// * `queue`        - Optional watcher for this range.
    ///
    /// # Errors
    ///
    /// Returns an error if the current page could not be fetched from the
    /// database.
    ///
    async fn page_at(&self, cursor_index: u32, tether: &Tether) -> Result<Vec<T>, StashError> {
        T::find(
            paging_query(&self.query_logic, cursor_index, self.page_size),
            convert_params(&self.params),
            tether,
        )
        .await
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the database.
    ///
    #[allow(clippy::missing_panics_doc)]
    pub async fn next_page(&self) -> Result<Vec<T>, R::Error> {
        // Acquire lock to prevent concurrent checks from the database
        // watcher until we are done updating all the relevant data.
        let mut shared = self.shared.lock().await;
        self.next_page_(&mut shared).await
    }

    #[allow(clippy::missing_panics_doc)]
    async fn next_page_(&self, shared: &mut Shared) -> Result<Vec<T>, R::Error> {
        let next_index = shared.cursor_index.saturating_add(self.page_size.get());
        let mut tether = self.stash.connection();
        let current_page = self.page_at(shared.cursor_index, &tether).await?;

        self.remote
            .sync_page_after(next_index, self.page_size, current_page, &mut tether)
            .await?;

        shared.cursor_index = next_index;
        shared.last_cursor_index = shared.cursor_index;
        // Get the first element of the next page to update the cursor id.
        if let Some(element) = T::find(
            paging_query(&self.query_logic, next_index, NonZeroU32::new(1).unwrap()),
            convert_params(&self.params),
            &tether,
        )
        .await?
        .into_iter()
        .next()
        {
            shared.cursor_row_id = Some(element.row_id().unwrap());
        }

        let next_page = self.page_at(shared.cursor_index, &tether).await?;
        Ok(next_page)
    }

    /// Moves to the previous page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page before the previous page could not be
    /// fetched from the database.
    ///
    pub async fn previous_page(&self) -> Result<Vec<T>, StashError> {
        let mut guard = self.shared.lock().await;
        guard.cursor_index = guard.cursor_index.saturating_sub(u32::from(self.page_size));
        guard.last_cursor_index = guard.cursor_index;
        drop(guard);
        self.current_page().await
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
    pub async fn reload(&self) -> Result<Vec<T>, R::Error> {
        self.handle_change().await
    }

    /// Retrieves the total number of records in the result set.
    pub async fn result_count(&self) -> u32 {
        self.shared.lock().await.row_count
    }

    /// Retrieves the current page number.
    pub async fn current_page_number(&self) -> u32 {
        #[allow(clippy::arithmetic_side_effects)]
        self.shared
            .lock()
            .await
            .cursor_index
            .saturating_div(u32::from(self.page_size))
            .saturating_add(1)
    }

    /// Retrieves the total number of pages.
    pub async fn page_count(&self) -> u32 {
        #[allow(clippy::arithmetic_side_effects)]
        self.shared
            .lock()
            .await
            .row_count
            .saturating_add(u32::from(self.page_size))
            .saturating_sub(1)
            .saturating_div(u32::from(self.page_size))
    }

    /// Checks if there is a next page available.
    pub async fn has_next_page(&self) -> bool {
        self.current_page_number().await < self.page_count().await
    }

    /// Checks if there is a previous page available.
    pub async fn has_previous_page(&self) -> bool {
        self.current_page_number().await > 1
    }

    /// Returns the current page size.
    #[must_use]
    pub const fn page_size(&self) -> NonZeroU32 {
        self.page_size
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
fn paging_query(query_logic: &str, cursor_index: u32, page_size: NonZeroU32) -> String {
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
