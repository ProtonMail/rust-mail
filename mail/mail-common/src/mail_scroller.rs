use crate::datatypes::{ReadFilter, SearchOptions};
use crate::models::{ConversationScrollData, MessageScrollData};
use crate::{MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_action_queue::action::Error;
use proton_core_common::datatypes::LocalLabelId;
use proton_task_service::AsyncTaskResult;
use stash::stash::WatcherHandle;
use std::sync::Weak;

mod mail_scroller_source;
mod mail_scroller_watcher;

pub use mail_scroller_source::*;
pub use mail_scroller_watcher::*;

#[cfg(test)]
#[path = "tests/mail_scroller/message_scroller.rs"]
mod message_scroller;

#[cfg(test)]
#[path = "tests/mail_scroller/conversation_scroller.rs"]
mod conversation_scroller;

/// Paginate over mail related items which implement [`MailScrollerSource`].
///
/// You should use [`has_more()`] to check if more data is available and [`fetch_more()`] to
/// retrieve the data from the server.
///
/// Whether the data is cached or always updated from the server, depends on the implementation
/// of [`MailScrollerSource`].
pub struct MailScroller<T: MailScrollerSource + 'static> {
    ctx: Weak<MailUserContext>,
    source: T,
    total: u64,
    task: MailPaginatorJoinHandle,
}

impl MailScroller<DataScrollerSource<ConversationScrollData>> {
    pub async fn conversations(
        ctx: Weak<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        previous_page_strategy: DataScrollerSourcePreviousPageStrategy,
    ) -> Result<Self, MailContextError> {
        let source =
            DataScrollerSource::new(local_label_id, unread, page_size, previous_page_strategy);
        MailScroller::new(ctx, source).await
    }
}

impl MailScroller<DataScrollerSource<MessageScrollData>> {
    pub async fn messages(
        ctx: Weak<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        previous_page_strategy: DataScrollerSourcePreviousPageStrategy,
    ) -> Result<Self, MailContextError> {
        let source =
            DataScrollerSource::new(local_label_id, unread, page_size, previous_page_strategy);
        MailScroller::new(ctx, source).await
    }
}

impl MailScroller<SearchScrollerSource> {
    pub async fn search(
        ctx: Weak<MailUserContext>,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
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
    async fn new(ctx: Weak<MailUserContext>, mut source: T) -> Result<Self, MailContextError> {
        let (total, task) = if let Some(ctx) = ctx.upgrade() {
            source.initialize(&ctx).await?
        } else {
            return Err(MailContextError::MissingContext);
        };

        Ok(Self {
            ctx,
            total,
            source,
            task,
        })
    }

    pub fn watch(&mut self) -> Result<WatcherHandle, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;

        let (sender, new_receiver) = flume::unbounded();
        let sender_clone = sender.clone();
        self.source.set_notify(sender);

        let WatcherHandle {
            receiver, handle, ..
        } = ctx.user_stash().subscribe_to(|sender| {
            Box::new(MailScrollerWatcher {
                sender,
                tables: self.source.watched_tables(),
            })
        })?;

        tokio::spawn(async move {
            while receiver.recv_async().await.is_ok() {
                if sender_clone.send_async(()).await.is_err() {
                    tracing::error!("MailScroller could not notify callback on database changes");
                    break;
                }
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
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;

        // If initialization is fetching something in the background, we wait
        // on that task to finish first.
        let previous_result = if self.task.is_some() {
            tracing::debug!("Awaiting for previous task");
            // Unwrap is safe here since we checked for `Some` above.
            self.task
                .take()
                .unwrap()
                .await
                .map_err(|_| MailContextError::Other(anyhow!("Failed to receive source data")))
                .and_then(|res| match res {
                    AsyncTaskResult::Completed(v) => v,
                    AsyncTaskResult::Cancelled => Err(MailContextError::TaskCancelled),
                })
        } else {
            Ok(())
        };

        let (items, new_total, task) = self
            .source
            .sync_next(&ctx)
            .await
            .inspect_err(|e| tracing::error!("Failed to fetch next page: {e:?}"))?;

        self.total = new_total;
        self.task = task;

        if items.is_empty() && self.seen().await? < self.total {
            if let Err(e) = previous_result {
                if e.is_network_failure() {
                    return Err(MailContextError::no_connection());
                }
            }
        }

        Ok(items)
    }

    /// Returns all the elements that are "visible" in the data source.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn all_items(&mut self) -> Result<Vec<T::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;

        self.total = self.source.all_items_total(&ctx).await?;

        self.source.visible_items(&ctx).await
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

        self.source.visible_items_total(&ctx).await
    }
}
