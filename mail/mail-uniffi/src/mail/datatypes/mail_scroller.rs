//! Paginator for managing large result sets with live updates.
//!
//! For more information, see the [`RealPaginator`] struct.
//!

use crate::errors::MailScrollerError;
use crate::mail::datatypes::Conversation;
use crate::{WatchHandle, async_runtime, uniffi_async};
use itertools::Itertools;
use proton_mail_common::datatypes::ReadFilter as RealReadFilter;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::mail_scroller::{
    DataScrollerSource, MailScroller as RealMailScroller, SearchScrollerSource,
};
use proton_mail_common::models::{ConversationScrollData, MessageScrollData};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::Message;

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq, Copy, uniffi::Enum)]
#[repr(u8)]
/// Conversation and message read filter.
pub enum ReadFilter {
    /// Return all messages/conversations.
    #[default]
    All = 0,
    /// Return only unread messages/conversations.
    Unread = 1,
    /// Return only read messages/conversations.
    Read = 2,
}

impl From<ReadFilter> for RealReadFilter {
    fn from(value: ReadFilter) -> Self {
        match value {
            ReadFilter::All => RealReadFilter::All,
            ReadFilter::Unread => RealReadFilter::Unread,
            ReadFilter::Read => RealReadFilter::Read,
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
#[derive(uniffi::Object)]
pub struct ConversationScroller {
    /// The "real" paginator that does the heavy lifting.
    pub(crate) scroller: Mutex<RealMailScroller<DataScrollerSource<ConversationScrollData>>>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,
}

#[uniffi_export]
impl ConversationScroller {
    /// Retrieves the handle to stop watching the data.
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
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
    pub async fn all_items(self: Arc<Self>) -> Result<Vec<Conversation>, MailScrollerError> {
        uniffi_async(async move {
            let mut scroller = self.scroller.lock().await;
            Result::<_, RealProtonMailError>::Ok(
                scroller.all_items().await?.into_iter().map_into().collect(),
            )
        })
        .await
        .map_err(MailScrollerError::from)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the API or database depending if it was already fetched.
    ///
    pub async fn fetch_more(self: Arc<Self>) -> Result<Vec<Conversation>, MailScrollerError> {
        uniffi_async(async move {
            let mut scroller = self.scroller.lock().await;
            Result::<_, RealProtonMailError>::Ok(
                scroller
                    .fetch_more()
                    .await?
                    .into_iter()
                    .map_into()
                    .collect(),
            )
        })
        .await
        .map_err(MailScrollerError::from)
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub fn total(&self) -> u64 {
        async_runtime().block_on(async { self.scroller.lock().await.total() })
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub fn has_more(&self) -> bool {
        async_runtime()
            .block_on(async { self.scroller.lock().await.has_more().await.unwrap_or(false) })
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
#[derive(uniffi::Object)]
pub struct MessageScroller {
    /// The "real" paginator that does the heavy lifting.
    pub(crate) scroller: Mutex<RealMailScroller<DataScrollerSource<MessageScrollData>>>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,
}

#[uniffi_export]
impl MessageScroller {
    /// Retrieves the handle to stop watching the data.
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
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
    pub async fn all_items(self: Arc<Self>) -> Result<Vec<Message>, MailScrollerError> {
        uniffi_async(async move {
            let mut scroller = self.scroller.lock().await;
            Result::<_, RealProtonMailError>::Ok(
                scroller.all_items().await?.into_iter().map_into().collect(),
            )
        })
        .await
        .map_err(MailScrollerError::from)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the API or database depending if it was already fetched.
    ///
    pub async fn fetch_more(self: Arc<Self>) -> Result<Vec<Message>, MailScrollerError> {
        uniffi_async(async move {
            let mut scroller = self.scroller.lock().await;
            Result::<_, RealProtonMailError>::Ok(
                scroller
                    .fetch_more()
                    .await?
                    .into_iter()
                    .map_into()
                    .collect(),
            )
        })
        .await
        .map_err(MailScrollerError::from)
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub fn total(&self) -> u64 {
        async_runtime().block_on(async { self.scroller.lock().await.total() })
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub fn has_more(&self) -> bool {
        async_runtime()
            .block_on(async { self.scroller.lock().await.has_more().await.unwrap_or(false) })
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
#[derive(uniffi::Object)]
pub struct SearchScroller {
    /// The "real" paginator that does the heavy lifting.
    pub(crate) scroller: Mutex<RealMailScroller<SearchScrollerSource>>,

    /// The handle to stop watching the data.
    pub(crate) handle: Arc<WatchHandle>,
}

#[uniffi_export]
impl SearchScroller {
    /// Retrieves the handle to stop watching the data.
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
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
    pub async fn all_items(self: Arc<Self>) -> Result<Vec<Message>, MailScrollerError> {
        uniffi_async(async move {
            let mut scroller = self.scroller.lock().await;
            Result::<_, RealProtonMailError>::Ok(
                scroller.all_items().await?.into_iter().map_into().collect(),
            )
        })
        .await
        .map_err(MailScrollerError::from)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// # Errors
    ///
    /// Returns an error if the page after the next page could not be fetched
    /// from the API or database depending if it was already fetched.
    ///
    pub async fn fetch_more(self: Arc<Self>) -> Result<Vec<Message>, MailScrollerError> {
        uniffi_async(async move {
            let mut scroller = self.scroller.lock().await;
            Result::<_, RealProtonMailError>::Ok(
                scroller
                    .fetch_more()
                    .await?
                    .into_iter()
                    .map_into()
                    .collect(),
            )
        })
        .await
        .map_err(MailScrollerError::from)
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub fn total(&self) -> u64 {
        async_runtime().block_on(async { self.scroller.lock().await.total() })
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub fn has_more(&self) -> bool {
        async_runtime()
            .block_on(async { self.scroller.lock().await.has_more().await.unwrap_or(false) })
    }
}
