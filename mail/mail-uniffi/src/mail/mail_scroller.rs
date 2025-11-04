use crate::core::datatypes::Id;
use crate::errors::MailScrollerError;
use crate::mail::datatypes::{Conversation, Message};
use crate::{PaginatorSearchOptions, WatchHandle, async_runtime, uniffi_async};
use proton_mail_common::MailUserContext;
use proton_mail_common::datatypes::{
    ContextualConversation as RealContextualConversation, IncludeSwitch as RealIncludeSwitch,
    ReadFilter as RealReadFilter,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::mail_cursor::{MailCursor as RealMailCursor, NextMailCursorItem};
use proton_mail_common::mail_scroller::{
    MailScroller as RealMailScroller, MailScrollerHandle, ScrollerListUpdate, ScrollerStatusUpdate,
    ScrollerUpdate,
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
    List(ConversationScrollerListUpdate),
    Status(ConversationScrollerStatusUpdate),
    /// An error has occurred.
    Error {
        error: MailScrollerError,
    },
}

#[derive(Debug, uniffi::Enum)]
pub enum ConversationScrollerStatusUpdate {
    FetchNewStart,
    FetchNewEnd,
}

impl From<ScrollerStatusUpdate> for ConversationScrollerStatusUpdate {
    fn from(status: ScrollerStatusUpdate) -> Self {
        match status {
            ScrollerStatusUpdate::FetchNewStart(_) => Self::FetchNewStart,
            ScrollerStatusUpdate::FetchNewEnd(_) => Self::FetchNewEnd,
        }
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum ConversationScrollerListUpdate {
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
}

impl From<ScrollerListUpdate<RealContextualConversation>> for ConversationScrollerListUpdate {
    fn from(update: ScrollerListUpdate<RealContextualConversation>) -> Self {
        match update {
            ScrollerListUpdate::None(_) => Self::None,
            ScrollerListUpdate::Append { src: _, items } => {
                Self::Append(items.into_iter().map(Conversation::from).collect())
            }
            ScrollerListUpdate::ReplaceFrom { src: _, idx, items } => Self::ReplaceFrom {
                idx: u64::try_from(idx).unwrap(),
                items: items.into_iter().map(Conversation::from).collect(),
            },
            ScrollerListUpdate::ReplaceBefore { src: _, idx, items } => Self::ReplaceBefore {
                idx: u64::try_from(idx).unwrap(),
                items: items.into_iter().map(Conversation::from).collect(),
            },
            ScrollerListUpdate::ReplaceRange {
                src: _,
                from,
                to,
                items,
            } => Self::ReplaceRange {
                from: u64::try_from(from).unwrap(),
                to: u64::try_from(to).unwrap(),
                items: items.into_iter().map(Conversation::from).collect(),
            },
        }
    }
}
impl From<ScrollerUpdate<RealContextualConversation>> for ConversationScrollerUpdate {
    fn from(update: ScrollerUpdate<RealContextualConversation>) -> Self {
        match update {
            ScrollerUpdate::List(update) => Self::List(update.into()),
            ScrollerUpdate::Error { src: _, error } => ConversationScrollerUpdate::Error {
                error: RealProtonMailError::from(error).into(),
            },
            ScrollerUpdate::Status(update) => Self::Status(update.into()),
        }
    }
}

pub(crate) fn spawn_conversation_scroller_watcher(
    user_ctx: &MailUserContext,
    handle: MailScrollerHandle<RealContextualConversation>,
    callback: Box<dyn ConversationScrollerLiveQueryCallback>,
) -> Arc<WatchHandle> {
    let MailScrollerHandle { updates, handle } = handle;

    let task = user_ctx.spawn(async move {
        let callback = Arc::new(callback);

        while let Ok(update) = updates.recv_async().await {
            let callback = callback.clone();

            _ = async_runtime()
                .spawn_blocking(move || callback.on_update(update.into()))
                .await;
        }
    });

    Arc::new(WatchHandle::new(handle, &task))
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
pub enum MessageScrollerStatusUpdate {
    FetchNewStart,
    FetchNewEnd,
}

impl From<ScrollerStatusUpdate> for MessageScrollerStatusUpdate {
    fn from(update: ScrollerStatusUpdate) -> Self {
        match update {
            ScrollerStatusUpdate::FetchNewStart(_) => Self::FetchNewStart,
            ScrollerStatusUpdate::FetchNewEnd(_) => Self::FetchNewEnd,
        }
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum MessageScrollerListUpdate {
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
}

impl From<ScrollerListUpdate<RealMessage>> for MessageScrollerListUpdate {
    fn from(update: ScrollerListUpdate<RealMessage>) -> Self {
        match update {
            ScrollerListUpdate::None(_) => Self::None,
            ScrollerListUpdate::Append { src: _, items } => {
                Self::Append(items.into_iter().map(Message::from).collect())
            }
            ScrollerListUpdate::ReplaceFrom { src: _, idx, items } => {
                Self::ReplaceFrom {
                    idx: u64::try_from(idx).unwrap(), // good luck not fitting in a u64
                    items: items.into_iter().map(Message::from).collect(),
                }
            }
            ScrollerListUpdate::ReplaceBefore { src: _, idx, items } => Self::ReplaceBefore {
                idx: u64::try_from(idx).unwrap(),
                items: items.into_iter().map(Message::from).collect(),
            },
            ScrollerListUpdate::ReplaceRange {
                src: _,
                from,
                to,
                items,
            } => Self::ReplaceRange {
                from: u64::try_from(from).unwrap(),
                to: u64::try_from(to).unwrap(),
                items: items.into_iter().map(Message::from).collect(),
            },
        }
    }
}

#[derive(Debug, uniffi::Enum)]
pub enum MessageScrollerUpdate {
    List(MessageScrollerListUpdate),
    Status(MessageScrollerStatusUpdate),
    /// An error has occurred.
    Error {
        error: MailScrollerError,
    },
}
impl From<ScrollerUpdate<RealMessage>> for MessageScrollerUpdate {
    fn from(value: ScrollerUpdate<RealMessage>) -> Self {
        match value {
            ScrollerUpdate::Status(update) => Self::Status(update.into()),
            ScrollerUpdate::List(update) => Self::List(update.into()),
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

    let task = user_ctx.spawn(async move {
        let callback = Arc::new(callback);

        while let Ok(update) = updates.recv_async().await {
            let callback = callback.clone();

            _ = async_runtime()
                .spawn_blocking(move || callback.on_update(update.into()))
                .await;
        }
    });

    Arc::new(WatchHandle::new(handle, &task))
}

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq, Copy, uniffi::Enum)]
#[repr(u8)]
pub enum ReadFilter {
    #[default]
    All = 0,
    Unread = 1,
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

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq, Copy, uniffi::Enum)]
#[repr(u8)]
pub enum IncludeSwitch {
    #[default]
    Default,
    WithSpamAndTrash,
}

impl From<IncludeSwitch> for RealIncludeSwitch {
    fn from(value: IncludeSwitch) -> Self {
        match value {
            IncludeSwitch::Default => RealIncludeSwitch::Default,
            IncludeSwitch::WithSpamAndTrash => RealIncludeSwitch::WithSpamAndTrash,
        }
    }
}

#[derive(uniffi::Object)]
pub struct ConversationScroller {
    scroller: Arc<RealMailScroller<RealContextualConversation>>,
    handle: Arc<WatchHandle>,
}

impl ConversationScroller {
    #[must_use]
    pub(crate) fn new(
        scroller: RealMailScroller<RealContextualConversation>,
        handle: Arc<WatchHandle>,
    ) -> Self {
        Self {
            scroller: Arc::new(scroller),
            handle,
        }
    }
}

#[uniffi_export]
impl ConversationScroller {
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
    }

    /// Forces a refresh of the scroller. The callback will receive the full
    /// list of items.
    pub fn force_refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .force_refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Refreshes the scroller, providing a smallest possible update
    /// to the client via the callback.
    pub fn refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Moves to the next page and retrieves its results.
    pub fn fetch_more(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_more(None)
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Tries to fetch the newest items.
    pub fn fetch_new(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_new()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the current items in the scroller, the items will be returned
    /// in the callback with the `ReplaceFrom { idx: 0, items }` update.
    pub fn get_items(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .get_items()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub fn change_filter(self: Arc<Self>, unread: ReadFilter) -> Result<(), MailScrollerError> {
        self.scroller
            .change_filter(unread.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub fn change_include(
        self: Arc<Self>,
        include: IncludeSwitch,
    ) -> Result<(), MailScrollerError> {
        self.scroller
            .change_include(include.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub async fn total(&self) -> Result<u64, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move { scroller.total().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    pub async fn has_more(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move { scroller.has_more().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    #[must_use]
    pub async fn cursor(
        &self,
        looking_at: Id,
    ) -> Result<Arc<MailConversationCursor>, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move {
            let cursor = scroller.cursor(looking_at.into()).await?;

            Result::<_, RealProtonMailError>::Ok(Arc::new(MailConversationCursor {
                cursor: Arc::new(cursor),
            }))
        })
        .await
        .map_err(Into::into)
    }

    #[must_use]
    pub async fn supports_include_filter(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move {
            scroller
                .supports_include_filter()
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(Into::into)
    }

    pub fn terminate(&self) {
        self.scroller.terminate();
    }
}

#[derive(uniffi::Object)]
pub struct MessageScroller {
    scroller: Arc<RealMailScroller<RealMessage>>,
    handle: Arc<WatchHandle>,
}

impl MessageScroller {
    #[must_use]
    pub(crate) fn new(scroller: RealMailScroller<RealMessage>, handle: Arc<WatchHandle>) -> Self {
        Self {
            scroller: Arc::new(scroller),
            handle,
        }
    }
}

#[uniffi_export]
impl MessageScroller {
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
    }

    /// Forces a refresh of the scroller. The callback will receive the full
    /// list of items.
    pub fn force_refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .force_refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Refreshes the scroller, providing a smallest possible update
    /// to the client via the callback.
    pub fn refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Moves to the next page and retrieves its results.
    pub fn fetch_more(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_more(None)
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Tries to fetch the newest items.
    pub fn fetch_new(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_new()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Changes the filter of the scroller.
    pub fn change_filter(self: Arc<Self>, unread: ReadFilter) -> Result<(), MailScrollerError> {
        self.scroller
            .change_filter(unread.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub fn change_include(
        self: Arc<Self>,
        include: IncludeSwitch,
    ) -> Result<(), MailScrollerError> {
        self.scroller
            .change_include(include.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the current items in the scroller, the items will be returned
    /// in the callback with the `ReplaceFrom { idx: 0, items }` update.
    pub fn get_items(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .get_items()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub async fn total(&self) -> Result<u64, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move { scroller.total().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    pub async fn has_more(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move { scroller.has_more().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    #[must_use]
    pub async fn cursor(
        &self,
        looking_at: Id,
    ) -> Result<Arc<MailMessageCursor>, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move {
            let cursor = scroller.cursor(looking_at.into()).await?;

            Result::<_, RealProtonMailError>::Ok(Arc::new(MailMessageCursor {
                cursor: Arc::new(cursor),
            }))
        })
        .await
        .map_err(Into::into)
    }

    #[must_use]
    pub async fn supports_include_filter(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move {
            scroller
                .supports_include_filter()
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(Into::into)
    }

    pub fn terminate(&self) {
        self.scroller.terminate();
    }
}

#[derive(uniffi::Object)]
pub struct SearchScroller {
    scroller: Arc<RealMailScroller<RealMessage>>,
    handle: Arc<WatchHandle>,
}

impl SearchScroller {
    #[must_use]
    pub(crate) fn new(scroller: RealMailScroller<RealMessage>, handle: Arc<WatchHandle>) -> Self {
        Self {
            scroller: Arc::new(scroller),
            handle,
        }
    }
}

#[uniffi_export]
impl SearchScroller {
    #[must_use]
    pub fn handle(&self) -> Arc<WatchHandle> {
        Arc::clone(&self.handle)
    }

    /// Forces a refresh of the scroller. The callback will receive the full
    /// list of items.
    pub fn force_refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .force_refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Refreshes the scroller, providing a smallest possible update
    /// to the client via the callback.
    pub fn refresh(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .refresh()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Moves to the next page and retrieves its results.
    pub fn fetch_more(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .fetch_more(None)
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub fn change_include(
        self: Arc<Self>,
        include: IncludeSwitch,
    ) -> Result<(), MailScrollerError> {
        self.scroller
            .change_include(include.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub fn change_keywords(
        self: Arc<Self>,
        keywords: PaginatorSearchOptions,
    ) -> Result<(), MailScrollerError> {
        self.scroller
            .change_keywords(keywords.into())
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    /// Retrieves the current items in the scroller, the items will be returned
    /// in the callback with the `ReplaceFrom { idx: 0, items }` update.
    pub fn get_items(self: Arc<Self>) -> Result<(), MailScrollerError> {
        self.scroller
            .get_items()
            .map_err(RealProtonMailError::from)
            .map_err(Into::into)
    }

    pub async fn total(&self) -> Result<u64, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move { scroller.total().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    pub async fn has_more(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move { scroller.has_more().await.map_err(RealProtonMailError::from) })
            .await
            .map_err(Into::into)
    }

    #[must_use]
    pub async fn cursor(
        &self,
        looking_at: Id,
    ) -> Result<Arc<MailMessageCursor>, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move {
            let cursor = scroller.cursor(looking_at.into()).await?;

            Result::<_, RealProtonMailError>::Ok(Arc::new(MailMessageCursor {
                cursor: Arc::new(cursor),
            }))
        })
        .await
        .map_err(Into::into)
    }

    #[must_use]
    pub async fn supports_include_filter(&self) -> Result<bool, MailScrollerError> {
        let scroller = Arc::clone(&self.scroller);

        uniffi_async(async move {
            scroller
                .supports_include_filter()
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(Into::into)
    }

    pub fn terminate(&self) {
        self.scroller.terminate();
    }
}

#[derive(uniffi::Object)]
pub struct MailConversationCursor {
    cursor: Arc<RealMailCursor<RealContextualConversation>>,
}

#[uniffi_export]
impl MailConversationCursor {
    pub fn peek_prev(&self) -> Option<Conversation> {
        self.cursor.peek_prev().map(Into::into)
    }

    #[must_use]
    pub fn peek_next(&self) -> NextMailCursorConversation {
        match self.cursor.peek_next() {
            NextMailCursorItem::None => NextMailCursorConversation::None,
            NextMailCursorItem::Some(item) => NextMailCursorConversation::Some(item.into()),
            NextMailCursorItem::Maybe => NextMailCursorConversation::Maybe,
        }
    }

    pub async fn fetch_next(&self) -> Result<Option<Conversation>, MailScrollerError> {
        let cursor = self.cursor.clone();

        uniffi_async(async move {
            cursor
                .fetch_next()
                .await
                .map(|item| item.map(Into::into))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(Into::into)
    }

    pub fn goto_prev(&self) {
        self.cursor.goto_prev();
    }

    pub fn goto_next(&self) {
        self.cursor.goto_next();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
#[allow(clippy::large_enum_variant)]
pub enum NextMailCursorConversation {
    None,
    Some(Conversation),
    Maybe,
}

#[derive(uniffi::Object)]
pub struct MailMessageCursor {
    cursor: Arc<RealMailCursor<RealMessage>>,
}

#[uniffi_export]
impl MailMessageCursor {
    pub fn peek_prev(&self) -> Option<Message> {
        self.cursor.peek_prev().map(Into::into)
    }

    #[must_use]
    pub fn peek_next(&self) -> NextMailCursorMessage {
        match self.cursor.peek_next() {
            NextMailCursorItem::None => NextMailCursorMessage::None,
            NextMailCursorItem::Some(item) => NextMailCursorMessage::Some(item.into()),
            NextMailCursorItem::Maybe => NextMailCursorMessage::Maybe,
        }
    }

    pub async fn fetch_next(&self) -> Result<Option<Message>, MailScrollerError> {
        let cursor = self.cursor.clone();

        uniffi_async(async move {
            cursor
                .fetch_next()
                .await
                .map(|item| item.map(Into::into))
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(Into::into)
    }

    pub fn goto_prev(&self) {
        self.cursor.goto_prev();
    }

    pub fn goto_next(&self) {
        self.cursor.goto_next();
    }
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
#[allow(clippy::large_enum_variant)]
pub enum NextMailCursorMessage {
    None,
    Some(Message),
    Maybe,
}
