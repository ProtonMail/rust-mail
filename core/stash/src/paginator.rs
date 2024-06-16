//! Paginator for managing large result sets with live updates.
//!
//! The [`Paginator`] struct provides a powerful and efficient way to manage
//! large result sets while supporting live updates. It is designed to work
//! seamlessly with the existing [`Model`] trait and [`Stash`] database
//! interface, offering intuitive pagination control.
//!
//! # Key features
//!
//!   1. **Sliding window**: Maintains a sliding window of pages, always keeping
//!      one page before the current page, one page after, and the first page of
//!      the result set in memory for quick access, in addition to the current
//!      page. The size of the window is adaptive, depending on updates to the
//!      result set.
//!
//!   2. **Live updates**: Supports real-time updates to the result set,
//!      handling insertions, updates, and deletions.
//!
//!   3. **Cursor management**: Keeps track of the current position in the
//!      result set, ensuring consistent navigation even as the underlying data
//!      changes.
//!
//!   4. **Efficient memory usage**: Stores a maximum of four pages in memory
//!      at any time. Also tracks the observed row IDs, but lazily, only when
//!      the rows in question have been loaded at some point.
//!
//!   5. **Asynchronous operations**: All database operations are asynchronous,
//!      preventing blocking.
//!
//! # How it works
//!
//! ## Initialisation
//!
//! When a [`Paginator`] is created, it:
//!
//!   1. Fetches the first two pages of results from the database.
//!   2. Sets up a listener for live updates (if requested)
//!   3. Initializes the sliding window with the fetched pages.
//!
//! ## Navigation
//!
//! As the client navigates through the result set:
//!
//!   - **Moving to the next page**
//!
//!       1. If the next page is not in memory, it's fetched from the database.
//!       2. The sliding window is updated, potentially dropping the earlier
//!          page.
//!       3. The cursor is moved forward.
//!
//!   - **Moving to the previous page**
//!
//!       1. If the previous page is not in memory, it's fetched from the
//!          database.
//!       2. The sliding window is updated, potentially dropping the latest
//!          page.
//!       3. The cursor is moved backward.
//!
//! ## Live updates
//!
//! The paginator listens for changes to the result set:
//!
//!   1. When a change occurs, it's processed immediately.
//!   2. The total count and cursor are updated as necessary.
//!   3. The change is applied to the in-memory pages if relevant.
//!   4. The change is sent to the client via a channel.
//!
//! ### Handling specific changes
//!
//!   - **Insertion**
//!   
//!       - If the new record belongs before the cursor, the cursor is moved
//!         forward.
//!       - The record is inserted into the correct position in the window.
//!   
//!   - **Update**
//!   
//!       - The record is updated in the cached data if it exists.
//!   
//!   - **Deletion**
//!   
//!       - If the deleted record was before the cursor, the cursor is moved
//!         backward.
//!       - The record is removed from the window if it exists.
//!       - If a page becomes empty, it's removed from the window.
//!
//! ## Cursor management
//!
//! The cursor represents the starting position of the current page in the
//! overall result set. It's adjusted as the client navigates and as live
//! updates occur, ensuring that the client's view of the data remains
//! consistent even as the underlying data changes.
//!
//! # Usage for clients
//!
//! Clients interact with the [`Paginator`] through a set of intuitive methods:
//!
//!   1. [`current_page()`](Paginator::current_page()):
//!      Get the current page. This will be obtained from the cache.
//!
//!   2. [`next_page()`](Paginator::next_page()):
//!      Move to and get the next page. These will be obtained from the cache,
//!      and the next page will be fetched from the database into the cache.
//!
//!   3. [`previous_page()`](Paginator::previous_page()):
//!      Move to and get the previous page. These will be obtained from the
//!      cache, and the previous page will be fetched from the database into the
//!      cache.
//!
//!   4. [`current_page_number()`](Paginator::current_page_number()):
//!      Get the current page number.
//!
//!   5. [`page_count()`](Paginator::page_count()):
//!      Get the total number of pages.
//!
//!   6. [`result_count()`](Paginator::row_count()):
//!      Get the total number of results.
//!
//!   7. [`has_next_page()`](Paginator::has_next_page()):
//!      Check if there's a next page available.
//!
//!   8. [`has_previous_page()`](Paginator::has_previous_page()):
//!      Check if there's a previous page available.
//!
//! # Example
//!
//! ```ignore
//! use stash::stash::{Stash, StashError};
//! use stash::orm::{Model, ResultsetChange};
//! use stash::paginator::PageControl;
//!
//! #[derive(Model)]
//! struct Email { /* ... */ }
//!
//! async fn example(stash: &Stash) -> Result<(), StashError> {
//! 	let (sender, receiver) = flume::unbounded::<ResultsetChange<Email, u64>>();
//!     let mut paginator = Email::find(&stash, "ORDER BY date DESC", vec![], PageControl{
//! 		page_number: 1,
//! 		page_size: 15,
//! 	}).await?;
//!     
//!     let first_page = paginator.current_page().await?;
//!     println!("First page: {:?}", first_page);
//!     
//!     let next_page = paginator.next_page().await?;
//!     println!("Second page: {:?}", next_page);
//! }
//! ```
//!

use crate::orm::{perform_find, Model, ResultsetChange};
use crate::stash::{AgnosticInterface, Interface, Stash, StashError};
use flume::Sender as QueueSender;
use indoc::formatdoc;
use rusqlite::ToSql;
use std::collections::HashMap;
use std::mem::take;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::spawn;
use tokio::sync::Mutex;
use tracing::error;
use uuid::Uuid;

/// Represents a set of cached page frames.
#[derive(Debug, Default)]
struct PageCache<T: Model> {
    /// The first page of records.
    first: Vec<T>,

    /// The previous page of records.
    previous: Vec<T>,

    /// The current page of records.
    current: Vec<T>,

    /// The next page of records.
    next: Vec<T>,
}

/// Pagination control parameters.
pub struct PageControl {
    /// The current page number. This is a 1-based index.
    pub page_number: NonZeroU32,

    /// The number of records per page. Note that pages are adaptive windows
    /// onto the result set, and so the actual number of records returned may
    /// vary from this value if the result set changes. The page size must not
    /// be zero — to disable pagination, do so on the [`Paginator`] itself.
    pub page_size: NonZeroU32,
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
pub struct Paginator<T: Model> {
    /// The current cursor position in the result set. This indicates the start
    /// of the current frame.
    cursor_index: Arc<Mutex<u32>>,

    /// Indicates whether pagination is active (`true`), or if all results were
    /// fetched (`false`).
    is_active: bool,

    /// The cached pages of records.
    page_cache: Arc<Mutex<PageCache<T>>>,

    /// The number of records per page. Assuming no changes to the result set,
    /// this will remain constant. However, if the data changes, the number for
    /// a particular page may vary.
    page_size: NonZeroU32,

    /// The parameters used in the query.
    params: Vec<Box<dyn ToSql + Send>>,

    /// The query logic used for finding records. This will be repeated when
    /// obtaining additional pages from the database.
    query_logic: String,

    /// The sending end of the queue for live updates to the result set, i.e.
    /// the end that the paginator uses to send changes to the client.
    queue: Option<QueueSender<ResultsetChange<T, T::IdType>>>,

    /// The IDs of the rows in the result set. Note that this is populated
    /// lazily, i.e. only when needed — it will grow over time, and is used to
    /// track changes to observed data.
    row_ids: Arc<Mutex<Vec<u64>>>,

    /// The total number of records in the result set. This will be kept updated
    /// as changes occur to the result set.
    row_count: Arc<Mutex<u32>>,

    /// The [`Stash`] instance used for database operations. This is not used
    /// for the initial query (that uses whatever was supplied), but is required
    /// for live updates.
    stash: Stash,

    /// The unique identifier for the view that this paginator is associated
    /// with. This is used to ensure that the paginator only listens for changes
    /// that are relevant to the current query.
    view_id: Uuid,
}

impl<T: Model> Paginator<T> {
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
    /// * `paging`      - The pagination options to use. If [`None`], all
    ///                   results will be fetched, and pagination will not be
    ///                   used or available.
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
        params: Vec<Box<dyn ToSql + Send>>,
        interface: &AgnosticInterface,
        paging: Option<PageControl>,
        queue: Option<QueueSender<ResultsetChange<T, T::IdType>>>,
    ) -> Result<Self, StashError>
    where
        Q: Into<String> + Send,
    {
        let view_id = Uuid::new_v4();
        let paginator = Self {
            cursor_index: Arc::new(Mutex::new(0)),
            is_active: paging.is_some(),
            page_cache: Arc::new(Mutex::new(PageCache {
                first: Vec::new(),
                previous: Vec::new(),
                current: Vec::new(),
                next: Vec::new(),
            })),
            page_size: if let Some(options) = paging {
                options.page_size
            } else {
                NonZeroU32::MAX
            },
            params,
            query_logic: query_logic.into(),
            queue: queue.clone(),
            row_count: Arc::new(Mutex::new(0)),
            row_ids: Arc::new(Mutex::new(Vec::new())),
            stash: interface.stash().clone(),
            view_id,
        };

        paginator.initialize(interface).await?;

        // We handle the queue ourselves, rather than relying on the one that
        // perform_find() manages, as that is not pagination-aware.
        if let Some(sender) = queue {
            paginator.start_update_listener(sender);
        }

        Ok(paginator)
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
    ///
    async fn initialize(&self, interface: &AgnosticInterface) -> Result<(), StashError> {
        let mut page_cache = self.page_cache.lock().await;
        if !self.is_active {
            let initial_records = perform_find(
                self.query_logic.clone(),
                self.params.clone(),
                &interface.clone().into(),
                self.queue.clone(),
            )
            .await?;

            *self.row_count.lock().await = initial_records.len() as u32;
            page_cache.first = initial_records;

            drop(page_cache);
            return Ok(());
        }

        let view_name = format!(
            "paginator_view_{}",
            self.view_id.to_string().replace("-", "_")
        );
        interface
            .execute(
                formatdoc!(
                    "
					CREATE VIEW
						{}
					AS SELECT
						{}.rowid AS rowid,
						*
					FROM
						{}
					{}
					",
                    view_name,
                    T::table_name(),
                    T::table_name(),
                    self.query_logic.clone()
                ),
                self.params.clone(),
            )
            .await?;
        let initial_records = interface
            .query(
                format!(
                    "SELECT * FROM {} LIMIT {}",
                    self.query_logic,
                    self.page_size.get() as usize * 2
                ),
                self.params.clone(),
            )
            .await?;

        *self.row_count.lock().await = initial_records.len() as u32;

        page_cache.first = initial_records
            .into_iter()
            .take(self.page_size.get() as usize)
            .collect();
        page_cache.next = initial_records
            .into_iter()
            .skip(self.page_size.get() as usize)
            .collect();

        drop(page_cache);
        Ok(())
    }

    /// Starts the update listener to handle live updates.
    fn start_update_listener(&self, sender: QueueSender<ResultsetChange<T, T::IdType>>) {
        let stash = self.stash.clone();
        let query_logic = self.query_logic.clone();
        let params = self.params.clone();
        let row_count = Arc::clone(&self.row_count);
        let cursor_index = Arc::clone(&self.cursor_index);
        let page_cache = Arc::clone(&self.page_cache);
        let view_id = self.view_id.clone();

        spawn(async move {
            // For now this is blanket subscriber — this will be optimised later to
            // only listen for changes that are relevant to the current query.
            if let Ok(mut receiver) = stash.subscribe().await {
                loop {
                    match receiver.recv_async().await {
                        Ok(notification) => {
                            if let Some(change) = T::handle_notification(
                                notification,
                                // We don't use this in the same way here
                                &mut HashMap::new(),
                                &stash,
                                &format!("paginator_view_{view_id}"),
                            )
                            .await
                            {
                                if let Err(e) = Self::handle_change(
                                    &change,
                                    &row_count,
                                    &cursor_index,
                                    &page_cache,
                                    &stash,
                                    &view_id.to_string(),
                                    &sender,
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
        });
    }

    /// Handles a change in the result set.
    ///
    /// This accepts references to shared state elements due to the listening
    /// loop not being able to operate on self.
    ///
    /// # Parameters
    ///
    /// * `change`       - The change that occurred in the result set.
    /// * `row_count`    - The total number of records in the result set.
    /// * `cursor_index` - The current cursor position in the result set.
    /// * `page_cache`   - The cached pages of records.
    /// * `stash`        - The [`Stash`] instance used for database operations.
    /// * `view_id`      - The unique identifier for the view.
    /// * `sender`       - The sender for live updates.
    ///
    async fn handle_change(
        change: &ResultsetChange<T, T::IdType>,
        row_count: &Arc<Mutex<u32>>,
        cursor_index: &Arc<Mutex<u32>>,
        page_cache: &Arc<Mutex<PageCache<T>>>,
        stash: &Stash,
        view_id: &str,
        sender: &flume::Sender<ResultsetChange<T, T::IdType>>,
    ) -> Result<(), StashError> {
        let view_name = format!("paginator_view_{}", view_id);
        let mut count = row_count.lock().await;
        let mut cursor = cursor_index.lock().await;
        let mut cache = page_cache.lock().await;

        match change {
            ResultsetChange::Inserted(record) | ResultsetChange::Updated(record) => {
                let position_query = format!(
                    "SELECT COUNT(*) FROM {} WHERE rowid <= (SELECT rowid FROM {} WHERE id = ?)",
                    view_name, view_name
                );
                let position: i64 = stash
                    .query_value(&position_query, vec![Box::new(record.id_value()?)])
                    .await?;

                Self::update_state_for_change(
                    record,
                    position as u32,
                    &mut count,
                    &mut cursor,
                    &mut cache,
                );
            }
            ResultsetChange::Deleted(id) => {
                Self::remove_record(id.clone(), &mut count, &mut cursor, &mut cache);
            }
        }

        // Notify the client of the change
        sender
            .send(change.clone())
            .map_err(|_| StashError::Custom("Failed to send update".into()))?;

        Ok(())
    }

    /// Updates the paginator's state when a record is inserted or updated.
    ///
    /// This function adjusts the total count, cursor position, and cached pages
    /// based on the position of the new or updated record in the result set.
    ///
    /// # Behaviour
    ///
    ///   - Increments the total count for insertions.
    ///   - Adjusts the cursor if the new/updated record is inserted before it.
    ///   - Updates the cached previous or next page if the record falls within
    ///     those ranges.
    ///   - Does not update the current page, as it's assumed to have already
    ///     been sent to the client.
    ///
    /// # Ordering
    ///
    /// The order of records is maintained as per the view, which reflects the
    /// original query's sorting criteria. No additional sorting is performed on
    /// the cached records.
    ///
    /// # Parameters
    ///
    /// * `record`       - The record that was inserted or updated.
    /// * `row_index`    - The position of the record in the overall result set
    ///                    (1-based index).
    /// * `row_count`    - Mutable reference to the total count of records.
    /// * `cursor_index` - Mutable reference to the cursor position.
    /// * `page_cache`   - Mutable reference to the [`PageCache`] containing
    ///                    cached pages.
    ///
    /// # Errors
    ///
    /// This function does not return any errors, because there would be no way
    /// to handle them in the context of a live update. Any errors are logged
    /// and ignored.
    ///
    fn update_state_for_change(
        record: &T,
        row_index: u32,
        row_count: &mut u32,
        cursor_index: &mut u32,
        page_cache: &mut PageCache<T>,
    ) {
        // Increment the total count for insertions
        *row_count += 1;

        // Adjust cursor if the new/updated record is before it
        if row_index <= *cursor_index {
            *cursor_index += 1;
        }

        // Update the cache
        if row_index < *cursor_index {
            // Record belongs in a page before the current one
            if !page_cache.previous.is_empty()
                && row_index >= *cursor_index - page_cache.previous.len() as u32
            {
                // Insert the record at the correct row_index
                let insert_index = page_cache.previous.len() - (*cursor_index - row_index) as usize;
                page_cache.previous.insert(insert_index, record.clone());
            }
        } else if row_index >= *cursor_index
            && row_index < *cursor_index + page_cache.next.len() as u32
        {
            // Record belongs in the next page
            let insert_index = (row_index - *cursor_index) as usize;
            page_cache.next.insert(insert_index, record.clone());
        }
        // Note: We don't update the current page as it's already been sent to the client
    }

    /// Removes a record from the paginator's state.
    ///
    /// This function adjusts the total count, cursor position, and cached pages
    /// when a record is deleted from the result set.
    ///
    /// # Behaviour
    ///
    ///   - Decrements the total count of records.
    ///   - Removes the record from the cached previous and next pages if
    ///     present.
    ///   - Adjusts the cursor if the deleted record was before it in the result
    ///     set.
    ///
    /// # Parameters
    ///
    /// * `id`         - The ID of the record to be removed.
    /// * `row_count`  - Mutable reference to the total count of records.
    /// * `cursor`     - Mutable reference to the cursor position.
    /// * `page_cache` - Mutable reference to the `PageCache` containing cached
    ///                  pages.
    ///
    fn remove_record(
        id: T::IdType,
        row_count: &mut u32,
        cursor_index: &mut u32,
        page_cache: &mut PageCache<T>,
    ) {
        // Decrement the total count
        *row_count = row_count.saturating_sub(1);

        // Remove from cache if present
        page_cache.previous.retain(|r| r.id_value().unwrap() != id);
        page_cache.next.retain(|r| r.id_value().unwrap() != id);

        // Adjust cursor if the deleted record was before it
        if page_cache.previous.len() < *cursor_index as usize {
            *cursor_index = cursor_index.saturating_sub(1);
        }
    }

    pub async fn cleanup(&self) -> Result<(), StashError> {
        self.stash
            .execute(
                format!(
                    "DROP VIEW IF EXISTS paginator_view_{}",
                    self.view_id.to_string().replace("-", "_")
                ),
                vec![],
            )
            .await?;
        Ok(())
    }

    /// Retrieves the results of the current page.
    pub async fn current_page(&mut self) -> Vec<T> {
        self.page_cache.lock().await.first.clone()
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the database.
    ///
    pub async fn next_page(&mut self) -> Result<Vec<T>, StashError> {
        let mut page_cache = self.page_cache.lock().await;
        let mut cursor = self.cursor_index.lock().await;

        if page_cache.next.is_empty() {
            let next_page = self.fetch_next_page().await?;
            page_cache.next = next_page;
        }

        page_cache.previous = take(&mut page_cache.current);
        page_cache.current = take(&mut page_cache.next);
        page_cache.next.clear();

        // TODO: Pre-fetch the next page here

        *cursor = u32::from(self.page_size);

        Ok(page_cache.current.clone())
    }

    /// Moves to the previous page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page before the previous page could not be
    /// fetched from the database.
    ///
    pub async fn previous_page(&mut self) -> Result<Vec<T>, StashError> {
        let mut page_cache = self.page_cache.lock().await;
        let mut cursor = self.cursor_index.lock().await;

        if *cursor == 0 {
            return Ok(page_cache.first.clone());
        }

        if page_cache.previous.is_empty() {
            let prev_page = self.fetch_previous_page().await?;
            page_cache.previous = prev_page;
        }

        page_cache.next = take(&mut page_cache.current);
        page_cache.current = take(&mut page_cache.previous);
        page_cache.previous.clear();

        // TODO: Pre-fetch the previous page here

        *cursor = cursor.saturating_sub(u32::from(self.page_size));

        Ok(page_cache.current.clone())
    }

    /// Fetches the next page of results from the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be fetched from the database.
    ///
    async fn fetch_next_page(&self) -> Result<Vec<T>, StashError> {
        let cursor = *self.cursor_index.lock().await;
        let query = format!(
            "{} LIMIT {} OFFSET {}",
            self.query_logic,
            self.page_size,
            cursor + u32::from(self.page_size) * 2
        );
        perform_find(
            &query,
            self.params.clone(),
            &self.stash.clone().into(),
            None,
        )
        .await
    }

    /// Fetches the previous page of results from the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be fetched from the database.
    ///
    async fn fetch_previous_page(&self) -> Result<Vec<T>, StashError> {
        let cursor = *self.cursor_index.lock().await;
        let query = format!(
            "{} LIMIT {} OFFSET {}",
            self.query_logic,
            self.page_size,
            cursor.saturating_sub(u32::from(self.page_size))
        );
        perform_find(
            &query,
            self.params.clone(),
            &self.stash.clone().into(),
            None,
        )
        .await
    }

    /// Retrieves the total number of records in the result set.
    pub async fn result_count(&self) -> u32 {
        *self.row_count.lock().await
    }

    /// Retrieves the current page number.
    pub async fn current_page_number(&self) -> u32 {
        (*self.cursor_index.lock().await / self.page_size) + 1
    }

    /// Retrieves the total number of pages.
    pub async fn page_count(&self) -> u32 {
        (*self.row_count.lock().await + u32::from(self.page_size) - 1) / self.page_size
    }

    /// Checks if there is a next page available.
    pub async fn has_next_page(&self) -> bool {
        self.current_page_number().await < self.page_count().await
    }

    /// Checks if there is a previous page available.
    pub async fn has_previous_page(&self) -> bool {
        self.current_page_number().await > 1
    }
}

impl<T: Model> Drop for Paginator<T> {
    fn drop(&mut self) {
        // Create a new runtime for cleanup
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = self.cleanup().await {
                eprintln!("Error during Paginator cleanup: {:?}", e);
            }
        });
    }
}
