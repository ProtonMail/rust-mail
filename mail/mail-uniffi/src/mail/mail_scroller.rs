use crate::core::datatypes::Id;
use crate::errors::MailScrollerError;
use crate::mail::datatypes::{Conversation, Message};
use crate::{WatchHandle, async_runtime, uniffi_async};
use itertools::Itertools;
use parking_lot::Mutex;
use proton_mail_common::MailUserContext;
use proton_mail_common::datatypes::{
    ContextualConversation, IncludeSwitch as RealIncludeSwitch, ReadFilter as RealReadFilter,
};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::mail_scroller::{
    MailScroller as RealMailScroller, MailScrollerHandle, ScrollerUpdate,
};
use proton_mail_common::models::Message as RealMessage;
use std::sync::Arc;
use tokio::sync::Notify;

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
) -> (Arc<WatchHandle>, Arc<MailboxList>) {
    let MailScrollerHandle { updates, handle } = handle;
    let mailbox_list = Arc::new(MailboxList::default());
    let mailbox_list_clone = mailbox_list.clone();
    let task_handle = user_ctx.spawn(async move {
        let callback = Arc::new(callback);

        while let Ok(update) = updates.recv_async().await {
            let callback = callback.clone();
            mailbox_list_clone.handle_update(&update);
            let update = update.into();
            let callback = move || callback.on_update(update);
            _ = async_runtime().spawn_blocking(callback).await;
        }
    });

    (
        Arc::new(WatchHandle::new(handle, &task_handle)),
        mailbox_list,
    )
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
) -> (Arc<WatchHandle>, Arc<MailboxList>) {
    let MailScrollerHandle { updates, handle } = handle;
    let mailbox_list = Arc::new(MailboxList::default());
    let mailbox_list_clone = mailbox_list.clone();
    let task_handle = user_ctx.spawn(async move {
        let callback = Arc::new(callback);

        while let Ok(update) = updates.recv_async().await {
            let callback = callback.clone();
            mailbox_list_clone.handle_update(&update);
            let update = update.into();
            let callback = move || callback.on_update(update);
            _ = async_runtime().spawn_blocking(callback).await;
        }
    });

    (
        Arc::new(WatchHandle::new(handle, &task_handle)),
        mailbox_list,
    )
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
    scroller: Arc<RealMailScroller>,
    handle: Arc<WatchHandle>,
    list: Arc<MailboxList>,
}

impl ConversationScroller {
    #[must_use]
    pub(crate) fn new(
        scroller: RealMailScroller,
        handle: Arc<WatchHandle>,
        list: Arc<MailboxList>,
    ) -> Self {
        Self {
            scroller: Arc::new(scroller),
            handle,
            list,
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
            .fetch_more()
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
    pub fn cursor(&self, index: u64) -> Arc<MailboxCursor> {
        Arc::new(MailboxCursor::new(
            index,
            self.scroller.clone(),
            self.list.clone(),
        ))
    }

    #[must_use]
    pub fn supports_include_filter(&self) -> bool {
        self.scroller.supports_include_filter()
    }

    pub fn terminate(&self) {
        self.scroller.terminate();
    }
}

#[derive(uniffi::Object)]
pub struct MessageScroller {
    scroller: Arc<RealMailScroller>,
    handle: Arc<WatchHandle>,
    list: Arc<MailboxList>,
}

impl MessageScroller {
    #[must_use]
    pub(crate) fn new(
        scroller: RealMailScroller,
        handle: Arc<WatchHandle>,
        list: Arc<MailboxList>,
    ) -> Self {
        Self {
            scroller: Arc::new(scroller),
            handle,
            list,
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
            .fetch_more()
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
    pub fn cursor(&self, index: u64) -> Arc<MailboxCursor> {
        Arc::new(MailboxCursor::new(
            index,
            self.scroller.clone(),
            self.list.clone(),
        ))
    }

    #[must_use]
    pub fn supports_include_filter(&self) -> bool {
        self.scroller.supports_include_filter()
    }

    pub fn terminate(&self) {
        self.scroller.terminate();
    }
}

#[derive(uniffi::Object)]
pub struct SearchScroller {
    scroller: Arc<RealMailScroller>,
    handle: Arc<WatchHandle>,
    list: Arc<MailboxList>,
}

impl SearchScroller {
    #[must_use]
    pub(crate) fn new(
        scroller: RealMailScroller,
        handle: Arc<WatchHandle>,
        list: Arc<MailboxList>,
    ) -> Self {
        Self {
            scroller: Arc::new(scroller),
            handle,
            list,
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
            .fetch_more()
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
    pub fn cursor(&self, index: u64) -> Arc<MailboxCursor> {
        Arc::new(MailboxCursor::new(
            index,
            self.scroller.clone(),
            self.list.clone(),
        ))
    }

    #[must_use]
    pub fn supports_include_filter(&self) -> bool {
        self.scroller.supports_include_filter()
    }

    pub fn terminate(&self) {
        self.scroller.terminate();
    }
}

#[derive(Default)]
pub(crate) struct MailboxList {
    items: Mutex<Vec<CursorEntry>>,
    notify: Notify,
}

impl MailboxList {
    fn get(&self, id: CursorEntryId, dir: CursorDirection) -> Option<CursorEntry> {
        let items = self.items.lock();

        if let Some(item) = items.iter().find(|item| item.id() == id.id) {
            Some(item.clone())
        } else {
            items
                .get(match dir {
                    CursorDirection::Forward => id.idx,
                    // TODO (ET-5057): This assumes that if the item is missing, then only one item was removed.
                    // Create a better approach to this problem and test it thoroughly.
                    CursorDirection::Backward => id.idx.checked_sub(1)?,
                })
                .cloned()
        }
    }

    fn siblings_at(&self, idx: usize) -> (PrevSibling, NextSibling) {
        Self::siblings_inner(&self.items.lock(), idx)
    }

    fn siblings_of(&self, id: CursorEntryId) -> (PrevSibling, NextSibling) {
        let items = self.items.lock();

        let idx = items
            .iter()
            .find_position(|item| item.id() == id.id)
            .map_or(id.idx, |(idx, _)| idx);

        Self::siblings_inner(&items, idx)
    }

    fn siblings_inner(items: &[CursorEntry], idx: usize) -> (PrevSibling, NextSibling) {
        let idx_to_id = |idx| {
            items.get(idx).map(|entry: &CursorEntry| CursorEntryId {
                id: entry.id(),
                idx,
            })
        };

        let prev = match idx.checked_sub(1).and_then(idx_to_id) {
            Some(prev) => PrevSibling::Some(prev),
            None => PrevSibling::None,
        };

        let next = match idx.checked_add(1).and_then(idx_to_id) {
            Some(next) => NextSibling::Some(next),
            None => match idx_to_id(idx) {
                Some(id) => NextSibling::Maybe(id),
                None => NextSibling::None,
            },
        };

        (prev, next)
    }

    async fn notify_when_changed(&self) {
        self.notify.notified().await;
    }

    fn handle_update<T>(&self, update: &ScrollerUpdate<T>)
    where
        T: Clone + Into<CursorEntry>,
    {
        match update {
            ScrollerUpdate::None(_) | ScrollerUpdate::Error { .. } => (),
            ScrollerUpdate::Append { src: _, items } => {
                self.items
                    .lock()
                    .extend(items.iter().map(|i| i.clone().into()));
            }
            ScrollerUpdate::ReplaceFrom { src: _, idx, items } => {
                self.items
                    .lock()
                    .splice(idx.., items.iter().map(|i| i.clone().into()));
            }
            ScrollerUpdate::ReplaceBefore { src: _, idx, items } => {
                self.items
                    .lock()
                    .splice(..idx, items.iter().map(|i| i.clone().into()));
            }
            ScrollerUpdate::ReplaceRange {
                src: _,
                from,
                to,
                items,
            } => {
                self.items
                    .lock()
                    .splice(from..to, items.iter().map(|i| i.clone().into()));
            }
        }

        self.notify.notify_waiters();
    }
}

#[derive(uniffi::Object)]
pub struct MailboxCursor {
    scroller: Arc<RealMailScroller>,
    siblings: Mutex<(PrevSibling, NextSibling)>,
    list: Arc<MailboxList>,
}

impl MailboxCursor {
    #[must_use]
    pub(crate) fn new(idx: u64, scroller: Arc<RealMailScroller>, list: Arc<MailboxList>) -> Self {
        let siblings = list.siblings_at(usize::try_from(idx).unwrap());

        Self {
            scroller,
            siblings: Mutex::new(siblings),
            list,
        }
    }
}

#[uniffi_export]
impl MailboxCursor {
    pub fn get_previous(&self) -> Result<Option<CursorEntry>, MailScrollerError> {
        match self.siblings.lock().0 {
            PrevSibling::None => Ok(None),
            PrevSibling::Some(id) => Ok(self.list.get(id, CursorDirection::Backward)),
        }
    }

    pub fn get_next(&self) -> Result<NextCursorEntry, MailScrollerError> {
        match self.siblings.lock().1 {
            NextSibling::None => Ok(NextCursorEntry::None),

            NextSibling::Some(id) => Ok(self
                .list
                .get(id, CursorDirection::Forward)
                .map_or(NextCursorEntry::None, NextCursorEntry::Some)),

            NextSibling::Maybe(_) => Ok(NextCursorEntry::CallAsync),
        }
    }

    pub async fn fetch_next(&self) -> Result<Option<CursorEntry>, MailScrollerError> {
        self.scroller
            .fetch_more()
            .map_err(RealProtonMailError::from)?;

        self.list.notify_when_changed().await;

        let mut siblings = self.siblings.lock();

        let NextSibling::Maybe(id) = siblings.1 else {
            return Ok(None);
        };

        *siblings = self.list.siblings_of(id);

        if let NextSibling::Some(id) = siblings.1 {
            Ok(self.list.get(id, CursorDirection::Forward))
        } else {
            Ok(None)
        }
    }

    pub fn go_forward(&self) {
        let mut siblings = self.siblings.lock();

        if let NextSibling::Some(id) = siblings.1 {
            *siblings = self.list.siblings_of(id);
        }
    }

    pub fn go_backward(&self) {
        let mut siblings = self.siblings.lock();

        if let PrevSibling::Some(id) = siblings.0 {
            *siblings = self.list.siblings_of(id);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CursorDirection {
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug)]
enum PrevSibling {
    None,
    Some(CursorEntryId),
}

#[derive(Clone, Copy, Debug)]
enum NextSibling {
    None,
    Some(CursorEntryId),
    Maybe(CursorEntryId),
}

#[derive(uniffi::Enum, Clone)]
pub enum CursorEntry {
    ConversationEntry(Conversation),
    MessageEntry(Message),
}

impl CursorEntry {
    fn id(&self) -> Id {
        match self {
            CursorEntry::ConversationEntry(conv) => conv.id,
            CursorEntry::MessageEntry(msg) => msg.id,
        }
    }
}

impl From<ContextualConversation> for CursorEntry {
    fn from(value: ContextualConversation) -> Self {
        CursorEntry::ConversationEntry(value.into())
    }
}

impl From<RealMessage> for CursorEntry {
    fn from(value: RealMessage) -> Self {
        CursorEntry::MessageEntry(value.into())
    }
}

#[derive(Clone, Copy, Debug)]
struct CursorEntryId {
    id: Id,
    idx: usize,
}

#[derive(uniffi::Enum)]
#[allow(clippy::large_enum_variant)]
pub enum NextCursorEntry {
    None,
    Some(CursorEntry),

    /// We don't know if there is anything or not,
    /// you should call fetch_next function which is asynchronous
    CallAsync,
}
