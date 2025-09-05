//! Paginator for managing large result sets with live updates.
//!
//! For more information, see the [`RealPaginator`] struct.
//!

use crate::errors::MailScrollerError;
use crate::mail::datatypes::{Conversation, Message};
use crate::{WatchHandle, async_runtime, uniffi_async};
use proton_mail_common::MailUserContext;
use proton_mail_common::datatypes::{ContextualConversation, ReadFilter as RealReadFilter};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::mail_scroller::{
    MailScroller as RealMailScroller, MailScrollerHandle, ScrollerUpdate,
};
use proton_mail_common::models::Message as RealMessage;
use std::sync::Arc;

/// A callback interface for live queries.
///
/// This interface is used to notify the client when observed data has been
/// updated.
///
#[uniffi::export(callback_interface)]
pub trait ConversationScrollerLiveQueryCallback: Send + Sync {
    /// Notify the client that the observed data has been updated.
    ///
    /// This method is called when the observed data has been updated. It
    /// provides the update to the client.
    ///
    /// Example of handling updates with the collected items vector (in Rust):
    ///
    /// ```ignore
    /// fn handle_updates(collected_items: &mut Vec<Conversation>, update: ConversationScrollerUpdate) {
    ///     match update {
    ///         ConversationScrollerUpdate::None => {
    ///             tracing::info!("No update has occurred.");
    ///         }
    ///         ConversationScrollerUpdate::Append(items) => {
    ///             collected_items.extend(items);
    ///         }
    ///         ConversationScrollerUpdate::ReplaceFrom { idx, items } => {
    ///             collected_items.splice(idx.., items);
    ///         }
    ///         ConversationScrollerUpdate::ReplaceBefore { idx, items } => {
    ///             collected_items.splice(..idx, items);
    ///         }
    ///         ConversationScrollerUpdate::ReplaceRange { from, to, items } => {
    ///             collected_items.splice(from..to, items);
    ///         }
    ///         ConversationScrollerUpdate::Error { error } => {
    ///             tracing::error!("Error: {}", error);
    ///         }
    ///     }
    /// }
    /// ```
    fn on_update(&self, update: ConversationScrollerUpdate);
}

#[derive(Debug, uniffi::Enum)]
pub enum ConversationScrollerUpdate {
    /// No update has occurred. It will be returned only for client-side requests.
    None,

    /// A new page of conversations needs to be appended to the end of the list.
    Append(Vec<Conversation>),

    /// A page of conversations needs to be replaced at the given index
    /// replacing everything forwards with the new items.
    /// Note: This replace includes the index while replacing
    ///
    /// # Examples
    /// [0, 1, 2, 3] -> ReplaceFrom { idx: 2, items: [5, 6, 7] } -> [0, 1, 5, 6, 7]
    /// [0, 1, 2, 3, 4, 8, 9] -> ReplaceFrom { idx: 2, items: [5, 6, 7] } -> [0, 1, 5, 6, 7]
    /// [0, 1, 2, 3] -> ReplaceFrom { idx: 0, items: [5, 6, 7] } -> [5, 6, 7]
    ReplaceFrom { idx: u64, items: Vec<Conversation> },

    /// A page of conversations needs to be replaced before the given index
    /// replacing everything before the index with the new items.
    /// Note: This replace excludes the index while replacing
    ///
    /// # Examples
    /// [0, 1, 2, 3] -> ReplaceBefore { idx: 2, items: [5, 6, 7] } -> [5, 6, 7, 3]
    /// [0, 1, 2, 3, 4, 8, 9] -> ReplaceBefore { idx: 2, items: [5, 6, 7] } -> [5, 6, 7, 3, 4, 8, 9]
    /// [0, 1, 2, 3] -> ReplaceBefore { idx: 0, items: [5, 6, 7] } -> [5, 6, 7, 0, 1, 2, 3]
    ReplaceBefore { idx: u64, items: Vec<Conversation> },

    /// A page of conversations needs to be replaced at the given index
    /// replacing everything between the given index and the end with the new items.
    /// [0, 1, 2, 3] -> ReplaceRange { from: 1, to: 3, items: [5, 6, 7] } -> [0, 5, 6, 7]
    /// [0, 1, 2, 3, 4, 8, 9] -> ReplaceRange { from: 1, to: 3, items: [5, 6, 7] } -> [0, 5, 6, 7, 4, 8, 9]
    /// [0, 1, 2, 3] -> ReplaceRange { from: 0, to: 2, items: [5, 6, 7] } -> [5, 6, 7, 3]
    /// [0, 1, 2, 3] -> ReplaceRange { from: 1, to: 1, items: [5, 6, 7] } -> [0, 5, 6, 7, 1, 2, 3]
    ///
    /// # Integration examples
    /// Rust: collected_items.splice(from..to, items)
    /// Swift: collected_items.replaceSubrange(from..<to, with: items)
    /// Kotlin: collected_items.subList(from, to).clear(); collected_items.addAll(from, items)
    ///
    ReplaceRange {
        from: u64, // inclusive
        to: u64,   // exclusive
        items: Vec<Conversation>,
    },

    /// An error has occurred.
    Error { error: MailScrollerError },
}

impl From<ScrollerUpdate<ContextualConversation>> for ConversationScrollerUpdate {
    fn from(update: ScrollerUpdate<ContextualConversation>) -> Self {
        match update {
            ScrollerUpdate::None(_) => ConversationScrollerUpdate::None,
            ScrollerUpdate::Append { src: _, items } => ConversationScrollerUpdate::Append(
                items.into_iter().map(Conversation::from).collect(),
            ),
            ScrollerUpdate::ReplaceFrom { src: _, idx, items } => {
                ConversationScrollerUpdate::ReplaceFrom {
                    idx: u64::try_from(idx).unwrap(),
                    items: items.into_iter().map(Conversation::from).collect(),
                }
            }
            ScrollerUpdate::ReplaceBefore { src: _, idx, items } => {
                ConversationScrollerUpdate::ReplaceBefore {
                    idx: u64::try_from(idx).unwrap(),
                    items: items.into_iter().map(Conversation::from).collect(),
                }
            }
            ScrollerUpdate::ReplaceRange {
                src: _,
                from,
                to,
                items,
            } => ConversationScrollerUpdate::ReplaceRange {
                from: u64::try_from(from).unwrap(),
                to: u64::try_from(to).unwrap(),
                items: items.into_iter().map(Conversation::from).collect(),
            },
            ScrollerUpdate::Error { src: _, error } => ConversationScrollerUpdate::Error {
                error: RealProtonMailError::from(error).into(),
            },
        }
    }
}

pub(crate) fn spawn_conversation_scroller_watcher(
    user_ctx: &MailUserContext,
    handle: MailScrollerHandle<ContextualConversation>,
    callback: Box<dyn ConversationScrollerLiveQueryCallback>,
) -> Arc<WatchHandle> {
    let MailScrollerHandle { updates, handle } = handle;
    let task_handle = user_ctx.spawn(async move {
        let callback = Arc::new(callback);

        while let Ok(update) = updates.recv_async().await {
            let callback = callback.clone();
            let callback = move || callback.on_update(update.into());
            _ = async_runtime().spawn_blocking(callback).await;
        }
    });

    Arc::new(WatchHandle::new(handle, &task_handle))
}

/// A callback interface for live queries.
///
/// This interface is used to notify the client when observed data has been
/// updated.
///
/// See [`ConversationScrollerLiveQueryCallback`] for additional examples.
#[uniffi::export(callback_interface)]
pub trait MessageScrollerLiveQueryCallback: Send + Sync {
    /// Notify the client that the observed data has been updated.
    ///
    /// This method is called when the observed data has been updated. It
    /// provides the update to the client.
    ///
    fn on_update(&self, update: MessageScrollerUpdate);
}

#[derive(Debug, uniffi::Enum)]
pub enum MessageScrollerUpdate {
    /// No update has occurred. It will be returned only for client-side requests.
    None,

    /// A new page of messages needs to be appended to the end of the list.
    Append(Vec<Message>),

    /// A page of messages needs to be replaced at the given index.
    /// Note: This replace includes the index while replacing
    ReplaceFrom { idx: u64, items: Vec<Message> },

    /// A page of messages needs to be replaced before the given index.
    /// Note: This replace excludes the index while replacing
    ReplaceBefore { idx: u64, items: Vec<Message> },

    /// A page of messages needs to be replaced at the given index
    /// replacing everything between the given index and the end with the new items.
    ReplaceRange {
        from: u64,
        to: u64,
        items: Vec<Message>,
    },

    /// An error has occurred.
    Error { error: MailScrollerError },
}

impl From<ScrollerUpdate<RealMessage>> for MessageScrollerUpdate {
    fn from(update: ScrollerUpdate<RealMessage>) -> Self {
        match update {
            ScrollerUpdate::None(_) => MessageScrollerUpdate::None,
            ScrollerUpdate::Append { src: _, items } => {
                MessageScrollerUpdate::Append(items.into_iter().map(Message::from).collect())
            }
            ScrollerUpdate::ReplaceFrom { src: _, idx, items } => {
                MessageScrollerUpdate::ReplaceFrom {
                    idx: u64::try_from(idx).unwrap(), // good luck not fitting in a u64
                    items: items.into_iter().map(Message::from).collect(),
                }
            }
            ScrollerUpdate::ReplaceBefore { src: _, idx, items } => {
                MessageScrollerUpdate::ReplaceBefore {
                    idx: u64::try_from(idx).unwrap(),
                    items: items.into_iter().map(Message::from).collect(),
                }
            }
            ScrollerUpdate::ReplaceRange {
                src: _,
                from,
                to,
                items,
            } => MessageScrollerUpdate::ReplaceRange {
                from: u64::try_from(from).unwrap(),
                to: u64::try_from(to).unwrap(),
                items: items.into_iter().map(Message::from).collect(),
            },
            ScrollerUpdate::Error { src: _, error } => MessageScrollerUpdate::Error {
                error: RealProtonMailError::from(error).into(),
            },
        }
    }
}

pub(crate) fn spawn_message_scroller_watcher(
    user_ctx: &MailUserContext,
    handle: MailScrollerHandle<RealMessage>,
    callback: Box<dyn MessageScrollerLiveQueryCallback>,
) -> Arc<WatchHandle> {
    let MailScrollerHandle { updates, handle } = handle;
    let task_handle = user_ctx.spawn(async move {
        let callback = Arc::new(callback);

        while let Ok(update) = updates.recv_async().await {
            let callback = callback.clone();
            let callback = move || callback.on_update(update.into());
            _ = async_runtime().spawn_blocking(callback).await;
        }
    });

    Arc::new(WatchHandle::new(handle, &task_handle))
}

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
    pub(crate) scroller: Arc<RealMailScroller>,

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

    /// Forces a refresh of the scroller. The callback will receive the full
    /// list of items.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn force_refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .force_refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Refreshes the scroller, providing a smallest possible update
    /// to the client via the callback.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn fetch_more(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_more()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Tries to fetch the newest items.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn fetch_new(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_new()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the current items in the scroller, the items will be returned
    /// in the callback with the `ReplaceFrom { idx: 0, items }` update.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn get_items(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .get_items()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Changes the filter of the scroller.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn change_filter(self: Arc<Self>, filter: ReadFilter) -> Result<(), MailScrollerError> {
        self.scroller
            .change_filter(filter.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub async fn total(&self) -> Result<u64, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);
        uniffi_async(async move { scroller.total().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub async fn has_more(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);
        uniffi_async(async move { scroller.has_more().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
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
    pub(crate) scroller: Arc<RealMailScroller>,

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

    /// Forces a refresh of the scroller. The callback will receive the full
    /// list of items.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn force_refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .force_refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Refreshes the scroller, providing a smallest possible update
    /// to the client via the callback.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn fetch_more(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_more()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Tries to fetch the newest items.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn fetch_new(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_new()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Changes the filter of the scroller.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn change_filter(self: Arc<Self>, filter: ReadFilter) -> Result<(), MailScrollerError> {
        self.scroller
            .change_filter(filter.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the current items in the scroller, the items will be returned
    /// in the callback with the `ReplaceFrom { idx: 0, items }` update.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn get_items(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .get_items()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub async fn total(&self) -> Result<u64, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);
        uniffi_async(async move { scroller.total().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub async fn has_more(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);
        uniffi_async(async move { scroller.has_more().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
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
    pub(crate) scroller: Arc<RealMailScroller>,

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

    /// Forces a refresh of the scroller. The callback will receive the full
    /// list of items.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn force_refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .force_refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Refreshes the scroller, providing a smallest possible update
    /// to the client via the callback.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Moves to the next page and retrieves its results.
    ///
    /// The call is non-blocking and returns immediately.
    pub fn fetch_more(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_more()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the total number of records in the result set.
    #[must_use]
    pub async fn total(&self) -> Result<u64, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);
        uniffi_async(async move { scroller.total().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    /// Checks if there is a next page available.
    #[must_use]
    pub async fn has_more(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);
        uniffi_async(async move { scroller.has_more().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }
}
