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

use crate::orm::{perform_find, Model, ResultsetChange};
use crate::stash::{AgnosticInterface, Interface, Stash, StashError};
use core::num::NonZeroU32;
use flume::Sender as QueueSender;
use indoc::formatdoc;
use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::{Error as SqliteError, ToSql};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::spawn;
use tokio::sync::Mutex;
use tracing::error;

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
pub struct Paginator<T: Model> {
    /// The current cursor position in the result set. This indicates the start
    /// of the current frame.
    cursor_index: Arc<Mutex<u32>>,

    /// The current row ID of the record at the cursor position in the result
    /// set. This is used to detect positional changes.
    #[allow(unused)]
    cursor_row_id: Arc<Mutex<u32>>,

    /// The number of records per page. Assuming no changes to the result set,
    /// this will remain constant. However, if the data changes, the number for
    /// a particular page may vary.
    page_size: NonZeroU32,

    /// The parameters used in the query.
    params: Vec<Param>,

    /// The query logic used for finding records. This will be repeated when
    /// obtaining additional pages from the database.
    query_logic: String,

    /// The sending end of the queue for live updates to the result set, i.e.
    /// the end that the paginator uses to send changes to the client.
    queue: Option<QueueSender<ResultsetChange<T, T::IdType>>>,

    /// The total number of records in the result set. This will be kept updated
    /// as changes occur to the result set.
    row_count: Arc<Mutex<u32>>,

    /// The [`Stash`] instance used for database operations. This is not used
    /// for the initial query (that uses whatever was supplied), but is required
    /// for subsequent queries and for live updates.
    stash: Stash,
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
    /// * `page_size`   - The number of records per page. Note that pages are
    ///                   adaptive windows onto the result set, and so the
    ///                   actual number of records returned may vary from this
    ///                   value if the result set changes. The page size must
    ///                   not be zero.
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
        page_size: NonZeroU32,
        queue: Option<QueueSender<ResultsetChange<T, T::IdType>>>,
    ) -> Result<Self, StashError>
    where
        Q: Into<String> + Send,
        A: Into<AgnosticInterface> + Interface,
    {
        let paginator = Self {
            cursor_index: Arc::new(Mutex::new(0)),
            cursor_row_id: Arc::new(Mutex::new(0)),
            page_size,
            params,
            query_logic: query_logic.into(),
            queue: queue.clone(),
            row_count: Arc::new(Mutex::new(0)),
            stash: interface.stash().clone(),
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
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records. Note that this will only be
    ///                 respected for the initial query, and not for any
    ///                 subsequent queries that are performed as a result of
    ///                 updates to the result set when pagination is active —
    ///                 those will use the underlying [`Stash`] instance.
    ///
    #[allow(clippy::cast_possible_truncation)]
    async fn initialize<A>(&self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let initial_records = perform_find(
            format!(
                "SELECT * FROM {} LIMIT {}",
                self.query_logic, self.page_size,
            ),
            convert_params(&self.params),
            &interface.clone().into(),
            self.queue.clone(),
        )
        .await?;

        // TODO: Call the API in the background to get the real number of
        // TODO: records. Also, if the initial result set is empty, call the API
        // TODO: in the foreground and wait for the first page to be returned.

        *self.row_count.lock().await = initial_records.len() as u32;

        Ok(())
    }

    /// Starts the update listener to handle live updates.
    fn start_update_listener(&self, sender: QueueSender<ResultsetChange<T, T::IdType>>) {
        let stash = self.stash.clone();
        let query_logic = self.query_logic.clone();
        let params = self.params.clone();
        let row_count = Arc::clone(&self.row_count);
        let cursor_index = Arc::clone(&self.cursor_index);
        let cursor_row_id = Arc::clone(&self.cursor_index);

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
                                    &row_count,
                                    &cursor_index,
                                    &cursor_row_id,
                                    &stash,
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
        row_count: &Arc<Mutex<u32>>,
        cursor_index: &Arc<Mutex<u32>>,
        cursor_row_id: &Arc<Mutex<u32>>,
        stash: &Stash,
        sender: &flume::Sender<ResultsetChange<T, T::IdType>>,
    ) -> Result<(), StashError> {
        #[allow(clippy::shadow_reuse)]
        let mut row_count = row_count.lock().await;
        #[allow(clippy::shadow_reuse)]
        let mut cursor_index = cursor_index.lock().await;
        #[allow(clippy::shadow_reuse)]
        let cursor_row_id = cursor_row_id.lock().await;

        match *change {
            ResultsetChange::Inserted(_) | ResultsetChange::Deleted(_) => {
                // Re-run the query to check if the cursor position needs to change. This
                // gets the first record at the offset of the cursor, and if doesn't have
                // the same ID as the current cursor record, we need to adjust the cursor.
                let cursor_record: Option<T> = T::find_first(
                    #[allow(clippy::unwrap_used)]
                    &paging_query(
                        T::table_name(),
                        query_logic,
                        *cursor_index,
                        NonZeroU32::new(1).unwrap(),
                    ),
                    convert_params(&params),
                    &stash.clone(),
                )
                .await?;

                match cursor_record {
                    Some(record) => {
                        #[allow(clippy::cast_lossless, clippy::unwrap_used)]
                        if *cursor_row_id as u64 != record.row_id().unwrap() {
                            // The change was made before the cursor position
                            if let ResultsetChange::Inserted(_) = *change {
                                *cursor_index = cursor_index.saturating_add(1);
                            } else if let ResultsetChange::Deleted(_) = *change {
                                *cursor_index = cursor_index.saturating_sub(1);
                            }
                        }
                    }
                    None => {
                        // We've reached the end of the result set, meaning a deletion before the
                        // cursor position
                        if let ResultsetChange::Deleted(_) = *change {
                            *cursor_index = cursor_index.saturating_sub(1);
                        }
                    }
                }

                // Update the total count
                if let ResultsetChange::Inserted(_) = *change {
                    *row_count = row_count.saturating_add(1);
                } else if let ResultsetChange::Deleted(_) = *change {
                    *row_count = row_count.saturating_sub(1);
                }
            }
            ResultsetChange::Updated(_) => {
                // No change to cursor or count for updates
            }
        }

        drop(row_count);
        drop(cursor_index);
        drop(cursor_row_id);

        // Notify the client of the change
        sender
            .send(change.clone())
            .map_err(|_err| StashError::Custom("Failed to send update".into()))?;

        Ok(())
    }

    /// Retrieves the results of the current page.
    ///
    /// # Errors
    ///
    /// Returns an error if the current page could not be fetched from the
    /// database.
    ///
    pub async fn current_page(&self) -> Result<Vec<T>, StashError> {
        perform_find(
            paging_query(
                T::table_name(),
                &self.query_logic,
                *self.cursor_index.lock().await,
                self.page_size,
            ),
            convert_params(&self.params),
            &self.stash.clone().into(),
            self.queue.clone(),
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
    pub async fn next_page(&self) -> Result<Vec<T>, StashError> {
        // TODO: Pre-fetch the next page here

        let mut cursor = self.cursor_index.lock().await;
        *cursor = cursor.saturating_add(u32::from(self.page_size));
        drop(cursor);
        self.current_page().await
    }

    /// Moves to the previous page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page before the previous page could not be
    /// fetched from the database.
    ///
    pub async fn previous_page(&self) -> Result<Vec<T>, StashError> {
        // TODO: Pre-fetch the previous page here

        let mut cursor = self.cursor_index.lock().await;
        *cursor = cursor.saturating_sub(u32::from(self.page_size));
        drop(cursor);
        self.current_page().await
    }

    /// Retrieves the total number of records in the result set.
    pub async fn result_count(&self) -> u32 {
        *self.row_count.lock().await
    }

    /// Retrieves the current page number.
    pub async fn current_page_number(&self) -> u32 {
        self.cursor_index
            .lock()
            .await
            .saturating_div(u32::from(self.page_size))
            .saturating_add(1)
    }

    /// Retrieves the total number of pages.
    pub async fn page_count(&self) -> u32 {
        #[allow(clippy::arithmetic_side_effects)]
        self.row_count
            .lock()
            .await
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
}

/// Constructs a query for paging through a result set.
///
/// # Parameters
///
/// * `tablename`    - The name of the table to query.
/// * `query_logic`  - The query logic to use for finding the records.
/// * `cursor_index` - The current cursor position in the result set.
/// * `page_size`    - The number of records per page.
///
fn paging_query(
    tablename: &str,
    query_logic: &str,
    cursor_index: u32,
    page_size: NonZeroU32,
) -> String {
    formatdoc!(
        "
            SELECT
                {}.rowid AS rowid, *
            FROM
                {}
            {}
            OFFSET
                {}
            LIMIT
                {}
        ",
        tablename,
        tablename,
        query_logic,
        cursor_index,
        page_size,
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
