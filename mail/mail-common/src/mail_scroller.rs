use crate::datatypes::{ContextualConversation, ReadFilter, SearchOptions};
use crate::models::{ConversationScrollData, Message, MessageScrollData};
use crate::{MailContextError, MailUserContext};
use anyhow::anyhow;
use derive_more::Display;
use futures::select;
use itertools::Itertools;
use proton_core_common::datatypes::LocalLabelId;
use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use stash::stash::WatcherHandle;
use std::sync::{Arc, Weak};
use tokio::sync::{RwLock, oneshot};
use tokio::task::AbortHandle;
use uuid::Uuid;

mod mail_scroller_source;
mod mail_scroller_watcher;

use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
pub use mail_scroller_source::*;
pub use mail_scroller_watcher::*;

#[cfg(test)]
#[path = "tests/mail_scroller/message_scroller.rs"]
mod message_scroller;

#[cfg(test)]
#[path = "tests/mail_scroller/conversation_scroller.rs"]
mod conversation_scroller;

#[derive(Debug, thiserror::Error)]
pub enum MailScrollerError {
    #[error("MailScroller is dirty, invalidating")]
    Dirty,
}

#[derive(Debug)]
pub enum ScrollerUpdate<T: Send + Sync + Clone + Eq + 'static> {
    None(ScrollerSource),
    Append {
        src: ScrollerSource,
        items: Vec<T>,
    },
    ReplaceFrom {
        src: ScrollerSource,
        idx: usize,
        items: Vec<T>,
    },
    ReplaceBefore {
        src: ScrollerSource,
        idx: usize,
        items: Vec<T>,
    },
    Error {
        src: ScrollerSource,
        error: MailContextError,
    },
}

impl<T: Send + Sync + Clone + Eq + 'static> ScrollerUpdate<T> {
    pub fn is_none(&self) -> bool {
        matches!(self, ScrollerUpdate::None(_))
    }

    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn src(&self) -> &ScrollerSource {
        match self {
            ScrollerUpdate::None(src) => src,
            ScrollerUpdate::Append { src, .. } => src,
            ScrollerUpdate::ReplaceFrom { src, .. } => src,
            ScrollerUpdate::ReplaceBefore { src, .. } => src,
            ScrollerUpdate::Error { src, .. } => src,
        }
    }

    pub fn is_scroll_event(&self) -> bool {
        matches!(self.src(), ScrollerSource::ScrollEvent(_))
    }
}

#[derive(Clone, Debug, Display)]
pub enum ScrollerSource {
    ScrollEvent(Uuid), // UUID of the scroll event
    Database,
    Invalidation,
}

/// We need to implement PartialEq to deduplicate commands in the ordered command queue.
/// This also means we cannot/should not use `Eq` for the enum.
/// If its needed then deduplication should be done in other way. This is because
/// we want to deduplicate commands that are not related to each other.
///
/// For example, if we have a command to fetch more data, we want to deduplicate it with
/// another command to fetch more data. But we do not want to deduplicate it with a command
/// to change the filter.
impl PartialEq for ScrollerSource {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Paginate over mail related items which implement [`MailScrollerSource`].
///
/// You should use [`has_more()`] to check if more data is available and [`fetch_more()`] to
/// retrieve the data from the server.
///
/// Whether the data is cached or always updated from the server, depends on the implementation
/// of [`MailScrollerSource`].
///
/// Dirty flag is used to indicate that the data is not up to date and needs to be
/// invalidated. It is set when the callback from the database is received and cleared
/// when the data is re-fetched using `all_items()`.
pub struct MailScroller {
    command: flume::Sender<ScrollerCommand>,
    ordered_command: flume::Sender<ScrollerOrderedCommand>,
    aborts: Vec<AbortHandle>,
}

impl Drop for MailScroller {
    fn drop(&mut self) {
        tracing::trace!(
            "Dropping MailScroller, aborting {} tasks",
            self.aborts.len()
        );

        for abort in self.aborts.drain(..) {
            abort.abort();
        }
    }
}

pub struct MailScrollerHandle<T: Send + Sync + Clone + Eq + 'static> {
    pub updates: flume::Receiver<ScrollerUpdate<T>>,
    pub handle: DropRemoveTableObserverHandle,
}

impl MailScroller {
    pub async fn conversations(
        ctx: Weak<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<ContextualConversation>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection();

        let order_dir = ScrollOrderDir::for_local_label(local_label_id, &tether).await?;
        let order_field = ScrollOrderField::for_local_label(local_label_id, &tether).await?;

        let source = DataScrollerSource::<ConversationScrollData>::new(
            local_label_id,
            unread,
            page_size,
            order_dir,
            order_field,
        );

        MailScroller::new(ctx, source, page_size).await
    }

    pub async fn messages(
        ctx: Weak<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<Message>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection();

        let order_dir = ScrollOrderDir::for_local_label(local_label_id, &tether).await?;
        let order_field = ScrollOrderField::for_local_label(local_label_id, &tether).await?;

        let source = DataScrollerSource::<MessageScrollData>::new(
            local_label_id,
            unread,
            page_size,
            order_dir,
            order_field,
        );

        MailScroller::new(ctx, source, page_size).await
    }

    pub async fn search(
        ctx: Weak<MailUserContext>,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<Message>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let source = SearchScrollerSource::new(search, page_size);

        MailScroller::new(ctx, source, page_size).await
    }

    async fn new<T: MailScrollerSource + 'static>(
        ctx: Arc<MailUserContext>,
        source: T,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<T::Item>), MailContextError> {
        let ctx = Arc::downgrade(&ctx);

        let ScrollerWorkerHandle {
            command,
            ordered_command,
            updates,
            handle,
            aborts,
        } = ScrollerWorker::run(ctx, source, page_size).await?;

        Ok((
            Self {
                command,
                ordered_command,
                aborts,
            },
            MailScrollerHandle { updates, handle },
        ))
    }

    pub async fn has_more(&self) -> Result<bool, MailContextError> {
        let (sender, receiver) = oneshot::channel();
        self.command
            .send(ScrollerCommand::HasMore(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send has more command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive has more response")))?
    }

    pub fn fetch_more(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();
        tracing::trace!("Sending `FetchMore` command with uuid: {uuid}");
        self.ordered_command
            .send(ScrollerOrderedCommand::FetchMore(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send fetch more command")))?;

        Ok(())
    }

    pub fn refresh(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();
        tracing::trace!("Sending `Refresh` command with uuid: {uuid}");
        self.ordered_command
            .send(ScrollerOrderedCommand::Refresh(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send refresh command")))?;

        Ok(())
    }

    pub fn force_refresh(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();
        tracing::trace!("Sending `ForceRefresh` command with uuid: {uuid}");
        self.ordered_command
            .send(ScrollerOrderedCommand::ForceRefresh(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send force refresh command"))
            })?;

        Ok(())
    }

    pub fn get_items(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();
        tracing::trace!("Sending `GetItems` command with uuid: {uuid}");
        self.ordered_command
            .send(ScrollerOrderedCommand::GetItems(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get items command")))?;

        Ok(())
    }

    pub fn change_filter(&self, filter: ReadFilter) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();
        tracing::trace!("Sending `ChangeFilter` command with uuid: {uuid}");
        self.ordered_command
            .send(ScrollerOrderedCommand::ChangeFilter {
                src: ScrollerSource::ScrollEvent(uuid),
                filter,
            })
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send change filter command"))
            })?;

        Ok(())
    }

    pub fn clear_cursor(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();
        tracing::trace!("Sending `ClearCursor` command with uuid: {uuid}");
        self.ordered_command
            .send(ScrollerOrderedCommand::ClearCursor(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send clear cursor command")))?;

        Ok(())
    }

    pub async fn total(&self) -> Result<u64, MailContextError> {
        let (sender, receiver) = oneshot::channel();
        self.command
            .send(ScrollerCommand::GetTotal(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get total command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive total response")))?
    }

    pub async fn seen(&self) -> Result<u64, MailContextError> {
        let (sender, receiver) = oneshot::channel();
        self.command
            .send(ScrollerCommand::GetSeen(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get seen command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive seen response")))?
    }

    pub async fn synced(&self) -> Result<u64, MailContextError> {
        let (sender, receiver) = oneshot::channel();
        self.command
            .send(ScrollerCommand::GetSynced(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get synced command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive synced response")))?
    }
}

pub struct ScrollerWorker<T: MailScrollerSource + 'static> {
    ctx: Weak<MailUserContext>,
    source: Arc<RwLock<T>>,
    task: MailPaginatorJoinHandle,
    items: Vec<T::Item>,
    page_size: usize,
    update: flume::Sender<ScrollerUpdate<T::Item>>,
    ordered_command: flume::Receiver<ScrollerOrderedCommand>,
}

impl<T: MailScrollerSource + 'static> ScrollerWorker<T> {
    async fn run(
        ctx: Weak<MailUserContext>,
        mut source: T,
        page_size: usize,
    ) -> Result<ScrollerWorkerHandle<T>, MailContextError> {
        let (update_sender, update_receiver) = flume::unbounded();
        let (command_sender, command_receiver) = flume::unbounded();
        let (ordered_command_sender, ordered_command_receiver) = flume::unbounded();
        let arc_ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let task = source.initialize(&arc_ctx).await?;
        let tables = source.watched_tables();

        let WatcherHandle {
            receiver: db_update,
            handle,
            ..
        } = arc_ctx
            .user_stash()
            .subscribe_to(move |sender| Box::new(MailScrollerWatcher { sender, tables }))?;

        let source = Arc::new(RwLock::new(source));
        let this = Self {
            ctx,
            source,
            page_size,
            task,
            items: vec![],
            update: update_sender,
            ordered_command: ordered_command_receiver,
        };

        let aborts = this.spawn(command_receiver, ordered_command_sender.clone(), db_update)?;

        Ok(ScrollerWorkerHandle {
            command: command_sender,
            ordered_command: ordered_command_sender,
            updates: update_receiver,
            handle,
            aborts,
        })
    }

    fn spawn(
        mut self,
        command_receiver: flume::Receiver<ScrollerCommand>,
        ordered_command_sender: flume::Sender<ScrollerOrderedCommand>,
        db_update: flume::Receiver<()>,
    ) -> Result<Vec<AbortHandle>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let mut aborts = vec![];
        let source_clone = self.source.clone();
        let weak_ctx = self.ctx.clone();
        let (invalidation_sender, invalidation_receiver) = flume::unbounded();

        // Ordered operations, these needs to be streamlined and not blocking other operations
        // thats why we are going to dedicate a separate task for them.
        let handle = ctx.spawn(async move {
            self.source.write().await.set_notify(invalidation_sender);
            while let Ok(command) = self.ordered_command.recv_async().await {
                // This prevents abusing the scroller by sending multiple commands
                // in a row. We do not want and need to handle all of them one by one.
                let commands = self.ordered_command.drain().collect_vec();
                tracing::trace!("Handling ordered commands: {:?}", commands);
                let mut processed = 0;
                for command in Some(command).into_iter().chain(commands).dedup() {
                    tracing::trace!("Processing ordered command: {:?}", command);
                    if let Err(e) = self.handle_ordered_command(command).await {
                        tracing::error!("Failed to handle ordered command: {e:?}");
                    }
                    processed += 1;
                }
                tracing::trace!("Processed {} ordered commands", processed);
            }
        });
        aborts.push(handle.abort_handle());

        // Unordered operations, database updates and data source invalidations
        let handle = ctx.spawn(async move {
            loop {
                select! {
                    r = invalidation_receiver.recv_async() => {
                        if let Err(e) = r {
                            tracing::error!("Failed to receive invalidation: {e:?}");
                            return;
                        }
                        let _ = ordered_command_sender
                            .send_async(ScrollerOrderedCommand::Refresh(ScrollerSource::Invalidation)).await
                            .inspect_err(|e| tracing::error!("Failed to send refresh command: {e:?}"));
                    }
                    r = db_update.recv_async() => {
                        if let Err(e) = r {
                            tracing::error!("Failed to receive db update: {e:?}");
                            return;
                        }
                        let _ = ordered_command_sender
                            .send_async(ScrollerOrderedCommand::Refresh(ScrollerSource::Database)).await
                            .inspect_err(|e| tracing::error!("Failed to send refresh command: {e:?}"));
                    }
                    r = command_receiver.recv_async() => {
                        if let Err(e) = r {
                            tracing::error!("Failed to receive command: {e:?}");
                            return;
                        }
                        if let Err(e) = Self::handle_command(r.unwrap(), &source_clone, &weak_ctx).await {
                            tracing::error!("Failed to handle command: {e:?}");
                        }
                    }
                }
            }
        });
        aborts.push(handle.abort_handle());

        Ok(aborts)
    }

    async fn handle_ordered_command(
        &mut self,
        command: ScrollerOrderedCommand,
    ) -> Result<(), MailContextError> {
        match command {
            ScrollerOrderedCommand::FetchMore(source) => {
                let result = self.fetch_more(source.clone()).await.unwrap_or_else(|e| {
                    ScrollerUpdate::Error {
                        src: source,
                        error: e,
                    }
                });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send(result)
                        .map_err(|e| anyhow!("Failed to send fetch more update: {e:?}"))?;
                }
            }
            ScrollerOrderedCommand::Refresh(source) => {
                let result = self
                    .refresh(false, source.clone())
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error {
                        src: source,
                        error: e,
                    });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send_async(result)
                        .await
                        .map_err(|e| anyhow!("Failed to send refresh update: {e:?}"))?;
                }
            }
            ScrollerOrderedCommand::ForceRefresh(source) => {
                let result = self
                    .refresh(true, source.clone())
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error {
                        src: source,
                        error: e,
                    });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send_async(result)
                        .await
                        .map_err(|e| anyhow!("Failed to send force refresh update: {e:?}"))?;
                }
            }
            ScrollerOrderedCommand::GetItems(src) => {
                let items_update = self.get_items(src.clone());

                self.update
                    .send_async(items_update)
                    .await
                    .map_err(|e| anyhow!("Failed to send get items update: {e:?}"))?;
            }
            ScrollerOrderedCommand::ChangeFilter { src, filter } => {
                let result = self
                    .change_filter(src.clone(), filter)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send_async(result)
                        .await
                        .map_err(|e| anyhow!("Failed to send change filter update: {e:?}"))?;
                }
            }
            ScrollerOrderedCommand::ClearCursor(src) => {
                let result = self
                    .clear_cursor(src.clone())
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                self.update
                    .send_async(result)
                    .await
                    .map_err(|e| anyhow!("Failed to send clear cursor update: {e:?}"))?;
            }
        }

        Ok(())
    }

    async fn handle_command(
        command: ScrollerCommand,
        source: &RwLock<T>,
        ctx: &Weak<MailUserContext>,
    ) -> Result<(), MailContextError> {
        match command {
            ScrollerCommand::GetTotal(sender) => {
                let total = Self::total(source, ctx).await;

                sender
                    .send(total)
                    .map_err(|e| anyhow!("Failed to send total: {e:?}"))?;
            }
            ScrollerCommand::GetSeen(sender) => {
                let seen = Self::seen(source, ctx).await;

                sender
                    .send(seen)
                    .map_err(|e| anyhow!("Failed to send seen: {e:?}"))?;
            }
            ScrollerCommand::GetSynced(sender) => {
                let synced = Self::synced(source, ctx).await;

                sender
                    .send(synced)
                    .map_err(|e| anyhow!("Failed to send synced: {e:?}"))?;
            }
            ScrollerCommand::HasMore(sender) => {
                let (total, seen) = (
                    Self::total(source, ctx).await,
                    Self::seen(source, ctx).await,
                );

                let has_more = match (total, seen) {
                    (Ok(total), Ok(seen)) => Ok(seen < total),
                    (Err(e), _) | (_, Err(e)) => Err(e),
                };

                sender
                    .send(has_more)
                    .map_err(|e| anyhow!("Failed to send has more: {e:?}"))?;
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, fields(src=%call_src))]
    async fn fetch_more(
        &mut self,
        call_src: ScrollerSource,
    ) -> Result<ScrollerUpdate<T::Item>, MailContextError> {
        let mut items = self.sync_next().await?;
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let (seen, synced, total, has_more_in_source) = {
            let source = self.source.read().await;
            let seen = source.seen_total(&ctx).await?;
            let synced = source.synced_total(&ctx).await?;
            let total = source.all_total(&ctx).await?;
            let has_more = source.has_more(&ctx).await?;
            (seen, synced, total, has_more)
        };
        let page_size = self.page_size as u64;
        let is_small_label = total > 0 && total < page_size;
        let has_more_in_label = seen < total;

        tracing::info!(
            "Fetch stats - seen/synced/total: {seen}/{synced}/{total}. Has more - source/label: {has_more_in_source}/{has_more_in_label}"
        );

        if items.is_empty() && has_more_in_label {
            if self.task.is_none() {
                // We will not progress any further without task,
                // and task will be spawned only when we are online,
                // lets wait for another call.
                return Err(MailContextError::no_connection());
            } else if is_small_label {
                // If we are on a small label, we can wait for the task
                // to complete and get requested data.
                // For other cases we would jump double pages.
                items = self.sync_next().await?;
            }
        }

        if items.is_empty() {
            tracing::debug!("No new items fetched");
            Ok(ScrollerUpdate::None(call_src))
        } else {
            tracing::debug!("New items fetched: {}", items.len());
            self.items.extend(items.clone());
            Ok(ScrollerUpdate::Append {
                src: call_src,
                items,
            })
        }
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn refresh(
        &mut self,
        force: bool,
        src: ScrollerSource,
    ) -> Result<ScrollerUpdate<T::Item>, MailContextError> {
        // Ensure small labels are refreshed before running diffs
        self.try_refresh_small_label(src.clone()).await?;

        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let visible_items = {
            let source = self.source.read().await;
            source.visible_items(&ctx).await?
        };

        tracing::info!(
            "Refresh stats - new count: {}, current count: {}",
            visible_items.len(),
            self.items.len()
        );

        let update = if force {
            self.items = visible_items.clone();

            ScrollerUpdate::ReplaceFrom {
                src,
                idx: 0,
                items: visible_items,
            }
        } else if self.items == visible_items {
            tracing::debug!("No update required");
            ScrollerUpdate::None(src)
        } else {
            tracing::debug!("Update is required, calculating diff...");
            let update = calculate_scroller_update(&self.items, &visible_items, src);
            self.items = visible_items;

            update
        };

        Ok(update)
    }

    fn get_items(&self, src: ScrollerSource) -> ScrollerUpdate<T::Item> {
        let items = self.items.clone();
        ScrollerUpdate::ReplaceFrom { src, idx: 0, items }
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn change_filter(
        &mut self,
        src: ScrollerSource,
        filter: ReadFilter,
    ) -> Result<ScrollerUpdate<T::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        tracing::debug!("Changing filter to {filter:?}");
        // We drop the task, we cannot await it in offline mode.
        let _ = self.task.take();
        self.source
            .write()
            .await
            .change_filter(&ctx, filter)
            .await?;
        self.items.clear();
        self.fetch_more(src.clone()).await?;
        self.refresh(true, src).await
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn clear_cursor(
        &mut self,
        src: ScrollerSource,
    ) -> Result<ScrollerUpdate<T::Item>, MailContextError> {
        tracing::info!("Clearing cursor for current label");
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        // We drop the task, we cannot await it in offline mode.
        let _ = self.task.take();
        self.source.write().await.clear_cursor(&ctx).await?;
        self.items.clear();
        self.fetch_more(src.clone()).await?;
        self.refresh(true, src).await
    }

    /// Return the total number of elements available.
    async fn total(
        source: &RwLock<T>,
        ctx: &Weak<MailUserContext>,
    ) -> Result<u64, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let source = source.read().await;
        source.all_total(&ctx).await
    }

    /// Return the number of already seen elements.
    async fn seen(
        source: &RwLock<T>,
        ctx: &Weak<MailUserContext>,
    ) -> Result<u64, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let source = source.read().await;
        source.seen_total(&ctx).await
    }

    /// Return the number of elements that have been synced.
    async fn synced(
        source: &RwLock<T>,
        ctx: &Weak<MailUserContext>,
    ) -> Result<u64, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let source = source.read().await;
        source.synced_total(&ctx).await
    }

    /// For small number of items or when the label was empty and now it has more items than a page
    /// we need to automatically fetch more to ensure that we have any new data.
    ///
    /// The logic is:
    /// - If the label has less than a page of items but is not empty, we might need to fetch more.
    /// - If the label was empty and now it suppose to have items, we might need to fetch more.
    /// - Now we need to check if we actually have any new items to fetch based on the synced count.
    ///
    async fn try_refresh_small_label(
        &mut self,
        src: ScrollerSource,
    ) -> Result<(), MailContextError> {
        let total = Self::total(&self.source, &self.ctx).await?;
        let page_size = self.page_size as u64;
        let is_small_label = total > 0 && total < page_size;

        if is_small_label {
            let seen = Self::seen(&self.source, &self.ctx).await?;
            let should_fetch = seen < total;
            if should_fetch {
                tracing::info!("Fetch more for small ({is_small_label}) label");
                if let Ok(result) = self.fetch_more(src).await
                    && result.is_some()
                {
                    let _ =
                        self.update.send_async(result).await.inspect_err(|e| {
                            tracing::error!("Failed to send append update: {e:?}")
                        });
                }
            }
        }

        Ok(())
    }

    async fn sync_next(&mut self) -> Result<Vec<T::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;

        if let Err(e) = self.wait_for_request().await {
            tracing::error!("Error occurred while waiting for previous request: {e:?}");
        }

        let (items, task) = {
            let mut source = self.source.write().await;
            source
                .sync_next(&ctx)
                .await
                .inspect_err(|e| tracing::error!("Failed to fetch next page: {e:?}"))?
        };

        tracing::debug!("Fetched next page, items number: {}", items.len());
        self.task = task;

        Ok(items)
    }

    async fn wait_for_request(&mut self) -> Result<(), MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let is_online = ctx.session().status().await.is_online();

        if self.task.is_some() && is_online {
            tracing::debug!("Awaiting for previous task");
            Self::await_task(&mut self.task).await
        } else {
            Ok(())
        }
    }

    async fn await_task(task: &mut MailPaginatorJoinHandle) -> Result<(), MailContextError> {
        tracing::debug!("Awaiting for previous task");

        if let Some(task) = task.take() {
            match task.await {
                Ok(result) => result,
                Err(_) => Err(MailContextError::TaskCancelled),
            }
        } else {
            Ok(())
        }
    }
}

struct ScrollerWorkerHandle<T: MailScrollerSource> {
    command: flume::Sender<ScrollerCommand>,
    ordered_command: flume::Sender<ScrollerOrderedCommand>,
    updates: flume::Receiver<ScrollerUpdate<T::Item>>,
    handle: DropRemoveTableObserverHandle,
    aborts: Vec<AbortHandle>,
}

enum ScrollerCommand {
    GetTotal(oneshot::Sender<Result<u64, MailContextError>>),
    GetSeen(oneshot::Sender<Result<u64, MailContextError>>),
    GetSynced(oneshot::Sender<Result<u64, MailContextError>>),
    HasMore(oneshot::Sender<Result<bool, MailContextError>>),
}

#[derive(Debug, Clone, PartialEq)]
enum ScrollerOrderedCommand {
    FetchMore(ScrollerSource),
    Refresh(ScrollerSource),
    ForceRefresh(ScrollerSource),
    GetItems(ScrollerSource),
    ChangeFilter {
        src: ScrollerSource,
        filter: ReadFilter,
    },
    ClearCursor(ScrollerSource),
}

fn calculate_scroller_update<T: Eq + Clone + Send + Sync + 'static>(
    old: &[T],
    new: &[T],
    src: ScrollerSource,
) -> ScrollerUpdate<T> {
    let prefix_count = || {
        old.iter()
            .zip(new.iter())
            .take_while(|(a, b)| a == b)
            .count()
    };

    // Items were removed, we need to replace from the beginning.
    if old.len() > new.len() {
        let idx = prefix_count();
        let items = new[idx..].to_vec();

        tracing::debug!("Replace from: {idx}, items number: {}", items.len());
        return ScrollerUpdate::ReplaceFrom { src, idx, items };
    }

    // Most updates come in from the beginning of the list in form of addition,
    // so when items were only added we can start counting from the end.
    let suffix_common_count = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();

    tracing::debug!("Common count from the end: {suffix_common_count}");
    let src_clone = src.clone();

    // For code reusability lets wrap this piece of logic in a closure.
    let replace_before = || {
        let idx = old.len().saturating_sub(suffix_common_count);
        let items = {
            // When index is 0, it means all items are common
            // and we need to insert new items to the beginning.
            // We need to calculate the index of the first new item.
            let idx = new.len().saturating_sub(suffix_common_count);
            new[..idx].to_vec()
        };
        tracing::debug!("Replace before: {idx}, items number: {}", items.len());
        ScrollerUpdate::ReplaceBefore {
            src: src_clone,
            idx,
            items,
        }
    };

    // Lets assume we will be happy when we have at least half in common.
    if suffix_common_count >= old.len() / 2 {
        replace_before()
    } else {
        // Otherwise compare with common items from the beginning.
        let prefix_common_count = prefix_count();
        tracing::debug!("Common count from the beginning: {prefix_common_count}");

        if suffix_common_count > prefix_common_count {
            replace_before()
        } else {
            let idx = prefix_common_count;
            let items = new[idx..].to_vec();
            tracing::debug!("Replace from: {idx}, items number: {}", items.len());
            ScrollerUpdate::ReplaceFrom { src, idx, items }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    // Helper function to create a test ScrollerSource
    fn test_source() -> ScrollerSource {
        ScrollerSource::ScrollEvent(Uuid::new_v4())
    }

    // Test cases for calculate_scroller_update function
    #[test_case(vec![], vec![] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items.is_empty(); "Test 1: empty to empty")]
    #[test_case(vec![], vec![1] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![1]; "Test 2: empty to single item")]
    #[test_case(vec![], vec![1, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![1, 2, 3]; "Test 3: empty to multiple items")]
    #[test_case(vec![1], vec![] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items.is_empty(); "Test 4: single item to empty")]
    #[test_case(vec![1, 2, 3], vec![] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items.is_empty(); "Test 5: multiple items to empty")]
    #[test_case(vec![1], vec![1] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items.is_empty(); "Test 6: same single item")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items.is_empty(); "Test 7: same multiple items")]
    // Items added at the beginning
    #[test_case(vec![1, 2, 3], vec![0, 1, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![0]; "Test 8: add one item at beginning")]
    #[test_case(vec![1, 2, 3], vec![0, -1, 1, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![0, -1]; "Test 9: add two items at beginning")]
    #[test_case(vec![3, 4, 5], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![1, 2]; "Test 10: add items at beginning with all suffix common")]
    // Items added at the end
    #[test_case(vec![1, 2, 3], vec![1, 2, 3, 4] => matches ScrollerUpdate::ReplaceFrom { idx: 3, items, .. } if items == vec![4]; "Test 11: add one item at end")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::ReplaceFrom { idx: 3, items, .. } if items == vec![4, 5]; "Test 12: add two items at end")]
    // Items added in the middle
    #[test_case(vec![1, 3], vec![1, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items == vec![1, 2]; "Test 13: add item in middle")]
    #[test_case(vec![1, 4], vec![1, 2, 3, 4] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items == vec![1, 2, 3]; "Test 14: add two items in middle")]
    // Items removed from beginning
    #[test_case(vec![1, 2, 3], vec![2, 3] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items == vec![2, 3]; "Test 15: remove one item from beginning")]
    #[test_case(vec![1, 2, 3, 4], vec![3, 4] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items == vec![3, 4]; "Test 16: remove two items from beginning")]
    // Items removed from end
    #[test_case(vec![1, 2, 3], vec![1, 2] => matches ScrollerUpdate::ReplaceFrom { idx: 2, items, .. } if items.is_empty(); "Test 17: remove one item from end")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 2] => matches ScrollerUpdate::ReplaceFrom { idx: 2, items, .. } if items.is_empty(); "Test 18: remove two items from end")]
    // Items removed from middle
    #[test_case(vec![1, 2, 3], vec![1, 3] => matches ScrollerUpdate::ReplaceFrom { idx: 1, items, .. } if items == vec![3]; "Test 19: remove item from middle")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 4] => matches ScrollerUpdate::ReplaceFrom { idx: 1, items, .. } if items == vec![4]; "Test 20: remove two items from middle")]
    // Items replaced
    #[test_case(vec![1, 2, 3], vec![1, 4, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 2, items, .. } if items == vec![1, 4]; "Test 21: replace item in middle")]
    #[test_case(vec![1, 2, 3], vec![4, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items == vec![4]; "Test 22: replace first item")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 4] => matches ScrollerUpdate::ReplaceFrom { idx: 2, items, .. } if items == vec![4]; "Test 23: replace last item")]
    // Completely different vectors
    #[test_case(vec![1, 2, 3], vec![4, 5, 6] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items == vec![4, 5, 6]; "Test 24: completely different same length")]
    #[test_case(vec![1, 2], vec![3, 4, 5] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items == vec![3, 4, 5]; "Test 25: completely different new longer")]
    #[test_case(vec![1, 2, 3], vec![4, 5] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items == vec![4, 5]; "Test 26: completely different new shorter")]
    // Complex cases that test the algorithm's logic
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![0, 1, 2, 3, 4, 5, 6] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![0]; "Test 27: add at beginning with many common suffix")]
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![1, 2, 3, 7, 8, 9] => matches ScrollerUpdate::ReplaceFrom { idx: 3, items, .. } if items == vec![7, 8, 9]; "Test 28: replace latter half")]
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![7, 8, 9, 4, 5, 6] => matches ScrollerUpdate::ReplaceBefore { idx: 3, items, .. } if items == vec![7, 8, 9]; "Test 29: replace first half")]
    // Edge cases with single elements
    #[test_case(vec![1], vec![1, 2] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items == vec![1, 2]; "Test 30: single to two elements")]
    #[test_case(vec![1, 2], vec![1] => matches ScrollerUpdate::ReplaceFrom { idx: 1, items, .. } if items.is_empty(); "Test 31: two to single element")]
    #[test_case(vec![1], vec![2] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items == vec![2]; "Test 32: single element replacement")]
    // Cases that test the 50% threshold logic
    #[test_case(vec![1, 2, 3, 4], vec![0, 2, 3, 4] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items == vec![0]; "Test 33: suffix common >= 50% triggers ReplaceBefore")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 0, 0, 0] => matches ScrollerUpdate::ReplaceFrom { idx: 1, items, .. } if items == vec![0, 0, 0]; "Test 34: prefix common > suffix common")]
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![0, 0, 0, 4, 5, 6] => matches ScrollerUpdate::ReplaceBefore { idx: 3, items, .. } if items == vec![0, 0, 0]; "Test 35: suffix wins over prefix")]
    // Large vectors to test performance characteristics
    #[test_case((1..=100).collect::<Vec<_>>(), (0..=100).collect::<Vec<_>>() => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![0]; "Test 36: large vector add at beginning")]
    #[test_case((1..=100).collect::<Vec<_>>(), (1..=101).collect::<Vec<_>>() => matches ScrollerUpdate::ReplaceFrom { idx: 100, items, .. } if items == vec![101]; "Test 37: large vector add at end")]

    // Test the actual function
    fn test_calculate_scroller_update(old: Vec<i32>, new: Vec<i32>) -> ScrollerUpdate<i32> {
        calculate_scroller_update(&old, &new, test_source())
    }

    #[test]
    fn test_scroller_source_is_preserved() {
        let src = ScrollerSource::Database;
        let result = calculate_scroller_update(&[1, 2], &[1, 2, 3], src.clone());

        match result {
            ScrollerUpdate::ReplaceFrom {
                src: result_src, ..
            } => {
                assert_eq!(result_src, src);
            }
            _ => panic!("Expected ReplaceFrom variant"),
        }
    }

    #[test]
    fn test_edge_case_all_common_suffix() {
        let old = vec![1, 2, 3, 4];
        let new = vec![0, 1, 2, 3, 4];
        let result = calculate_scroller_update(&old, &new, test_source());

        match result {
            ScrollerUpdate::ReplaceBefore { idx, items, .. } => {
                assert_eq!(idx, 0); // All old items are common suffix, so replace before idx 0
                assert_eq!(items, vec![0]); // Only the new item at the beginning
            }
            _ => panic!("Expected ReplaceBefore variant"),
        }
    }

    #[test]
    fn test_edge_case_no_common_elements() {
        let old = vec![1, 2, 3];
        let new = vec![4, 5, 6, 7];
        let result = calculate_scroller_update(&old, &new, test_source());

        match result {
            ScrollerUpdate::ReplaceFrom { idx, items, .. } => {
                assert_eq!(idx, 0); // No common prefix
                assert_eq!(items, vec![4, 5, 6, 7]); // All new items
            }
            _ => panic!("Expected ReplaceFrom variant"),
        }
    }

    #[test]
    fn test_complex_scenario_mixed_changes() {
        // Simulate a real-world scenario: some items added at beginning, some modified
        let old = vec![10, 20, 30, 40, 50];
        let new = vec![5, 15, 20, 35, 40, 50]; // Added 5, changed 10->15, changed 30->35
        let result = calculate_scroller_update(&old, &new, test_source());

        match result {
            ScrollerUpdate::ReplaceBefore { idx, items, .. } => {
                assert_eq!(idx, 3); // Common suffix: [40, 50] starting at idx 3 in old
                assert_eq!(items, vec![5, 15, 20, 35]); // New items before the common suffix
            }
            _ => panic!("Expected ReplaceBefore variant for this scenario"),
        }
    }
}
