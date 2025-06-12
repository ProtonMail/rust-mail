use crate::datatypes::{ReadFilter, SearchOptions};
use crate::models::{ConversationScrollData, MessageScrollData};
use crate::{MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_core_common::datatypes::LocalLabelId;
use proton_task_service::AsyncTaskResult;
use stash::stash::WatcherHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

mod mail_scroller_source;
mod mail_scroller_watcher;

use crate::datatypes::labels::LabelScrollOrder;
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
pub struct MailScroller<T: MailScrollerSource + 'static> {
    ctx: Weak<MailUserContext>,
    source: Arc<Mutex<T>>,
    total: u64,
    task: MailPaginatorJoinHandle,
    dirty: Arc<AtomicBool>,
}

impl MailScroller<DataScrollerSource<ConversationScrollData>> {
    pub async fn conversations(
        ctx: Weak<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let scroll_order =
            LabelScrollOrder::for_local_label_id(local_label_id, &ctx.user_stash().connection())
                .await?;
        let source = DataScrollerSource::new(local_label_id, unread, page_size, scroll_order);
        MailScroller::new(ctx, source).await
    }
}

impl MailScroller<DataScrollerSource<MessageScrollData>> {
    pub async fn messages(
        ctx: Weak<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let scroll_order =
            LabelScrollOrder::for_local_label_id(local_label_id, &ctx.user_stash().connection())
                .await?;
        let source = DataScrollerSource::new(local_label_id, unread, page_size, scroll_order);
        MailScroller::new(ctx, source).await
    }
}

impl MailScroller<SearchScrollerSource> {
    pub async fn search(
        ctx: Weak<MailUserContext>,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let source = SearchScrollerSource::new(search, page_size);
        MailScroller::new(ctx, source).await
    }
}

impl<T: MailScrollerSource> MailScroller<T> {
    /// Create a new instance with the `source` and the maximum `element_count` of elements
    /// that should be retrieved from the server on each request.
    ///
    /// # Errors
    ///
    /// Returns error if something went wrong with initializing the data source.
    async fn new(ctx: Arc<MailUserContext>, mut source: T) -> Result<Self, MailContextError> {
        let (total, task) = source.initialize(&ctx).await?;

        Ok(Self {
            ctx: Arc::downgrade(&ctx),
            total,
            source: Arc::new(Mutex::new(source)),
            task,
            dirty: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn watch(&mut self) -> Result<WatcherHandle, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;

        let (sender, new_receiver) = flume::unbounded();
        let sender_clone = sender.clone();
        let mut src = self.source.lock().await;
        let tables = src.watched_tables();
        src.set_notify(sender);
        drop(src);
        let weak_src = Arc::downgrade(&self.source);
        let dirty = self.dirty.clone();

        let WatcherHandle {
            receiver, handle, ..
        } = ctx
            .user_stash()
            .subscribe_to(move |sender| Box::new(MailScrollerWatcher { sender, tables }))?;

        tokio::spawn(async move {
            while receiver.recv_async().await.is_ok() {
                let Some(src) = weak_src.upgrade() else {
                    tracing::warn!("MailScroller source dropped, despawn watcher");
                    break;
                };

                // Make sure source is free to be used
                let _guard = src.lock().await;
                dirty.store(true, Ordering::Release);
                if sender_clone.send_async(()).await.is_err() {
                    tracing::error!("MailScroller could not notify callback on database changes");
                    break;
                }
                tracing::trace!("MailScroller notified about database changes");
            }
            tracing::warn!("MailScroller receiver closed, despawn watcher");
        });

        Ok(WatcherHandle::new(new_receiver, handle))
    }

    /// Check whether there is more data available.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn has_more(&self) -> Result<bool, MailContextError> {
        // We can't cache the visible item count since the data returned
        // via the API may not always line up correctly (e.g.: Conversations) and
        // external event updates.
        // We could use our own table observer to be notified of changes
        // but we may as well check the source for the final "truth".
        let visible_items = self.seen().await?;
        Ok(visible_items < self.total)
    }

    /// Fetch more data from the server.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be fetched or saved.
    pub async fn fetch_more(&mut self) -> Result<Vec<T::Item>, MailContextError> {
        // The check is done before acquiring the lock as if the `all_items` call is ongoing
        // we can pass through the dirty flag and when the lock will be acquired it will
        // return correct next page.
        if self.dirty.load(Ordering::Acquire) {
            return Err(MailScrollerError::Dirty.into());
        }

        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let mut src = self.source.lock().await;
        let is_online = ctx.session().status().await.is_online();

        // If initialization is fetching something in the background,
        // we want to wait for it if we have network to do so.
        let previous_result = if self.task.is_some() && is_online {
            Self::await_task(&mut self.task).await
        } else {
            Ok(())
        };

        let (items, new_total, task) = src
            .sync_next(&ctx)
            .await
            .inspect_err(|e| tracing::error!("Failed to fetch next page: {e:?}"))?;

        self.total = new_total;
        self.task = task;

        let seen = src.visible_items_total(&ctx).await?;

        if items.is_empty() && seen < self.total {
            previous_result?;

            if self.task.is_none() {
                // We will not progress any further without task,
                // and task will be spawned only when we are online,
                // lets wait for another call.
                return Err(MailContextError::no_connection());
            }
        }

        drop(src);

        Ok(items)
    }

    /// Returns all the elements that are "visible" in the data source.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn all_items(&mut self) -> Result<Vec<T::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        // We need to acquire the lock before clearing the dirty flag
        // as the fetch more should not be able to pass through the dirty flag.
        // before all_items is finished loading.
        let src = self.source.lock().await;

        self.dirty.store(false, Ordering::Release);
        self.total = src.all_items_total(&ctx).await?;
        let items = src.visible_items(&ctx).await;

        drop(src);

        items
    }

    /// Return the total number of elements available.
    ///
    /// Note: This value does not react to changes until more
    /// data is fetched from the server.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Return the number of already seen elements.
    pub async fn seen(&self) -> Result<u64, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let src = self.source.lock().await;
        let total = src.visible_items_total(&ctx).await;

        drop(src);

        total
    }

    async fn await_task(task: &mut MailPaginatorJoinHandle) -> Result<(), MailContextError> {
        tracing::debug!("Awaiting for previous task");

        if task.is_some() {
            task.take()
                .unwrap()
                .await
                .map_err(|_| MailContextError::Other(anyhow!("Failed to receive source data")))
                .and_then(|res| match res {
                    AsyncTaskResult::Completed(v) => v,
                    AsyncTaskResult::Cancelled => Err(MailContextError::TaskCancelled),
                })
        } else {
            Ok(())
        }
    }
}
