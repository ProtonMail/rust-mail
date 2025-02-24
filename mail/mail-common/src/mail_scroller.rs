use crate::datatypes::{ReadFilter, SearchOptions};
use crate::models::{ConversationScrollData, MessageScrollData};
use crate::{MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_core_common::async_task::AsyncTaskResult;
use proton_core_common::datatypes::LocalLabelId;
use stash::stash::{StashError, WatcherHandle};
use std::sync::Arc;

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
    ctx: Arc<MailUserContext>,
    source: T,
    total: u64,
    task: MailPaginatorJoinHandle,
}

impl MailScroller<DataScrollerSource<ConversationScrollData>> {
    pub async fn conversations(
        ctx: Arc<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let source = DataScrollerSource::new(local_label_id, unread, page_size);
        MailScroller::new(ctx, source).await
    }
}

impl MailScroller<DataScrollerSource<MessageScrollData>> {
    pub async fn messages(
        ctx: Arc<MailUserContext>,
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let source = DataScrollerSource::new(local_label_id, unread, page_size);
        MailScroller::new(ctx, source).await
    }
}

impl MailScroller<SearchScrollerSource> {
    pub async fn search(
        ctx: Arc<MailUserContext>,
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
    async fn new(ctx: Arc<MailUserContext>, mut source: T) -> Result<Self, MailContextError> {
        let (total, task) = source.initialize(&ctx).await?;

        Ok(Self {
            ctx,
            total,
            source,
            task,
        })
    }

    pub fn watch(&mut self) -> Result<WatcherHandle, StashError> {
        let (sender, new_receiver) = flume::unbounded();
        let sender_clone = sender.clone();
        self.source.set_notify(sender);

        let WatcherHandle {
            receiver, handle, ..
        } = self.ctx.user_stash().subscribe_to(|sender| {
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
        // If initialization is fetching something in the background, we wait
        // on that task to finish first.
        let is_online = self.ctx.session().status().await.is_online();
        let result = if self.task.is_some() && is_online {
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

        let (items, new_total, task) = self.source.sync_next(&self.ctx).await?;
        self.total = new_total;
        self.task = task;

        if result.is_err() && is_online {
            tracing::error!("Failed to fetch next page in the background: {:?}", result);

            if items.is_empty() {
                result?;
            }

            Ok(items)
        } else if items.is_empty() && !is_online && self.seen().await? < self.total {
            Err(MailContextError::no_connection())
        } else {
            Ok(items)
        }
    }

    /// Returns all the elements that are "visible" in the data source.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn all_items(&mut self) -> Result<Vec<T::Item>, MailContextError> {
        self.total = self.source.all_items_total(&self.ctx).await?;

        self.source.visible_items(&self.ctx).await
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
        self.source.visible_items_total(&self.ctx).await
    }
}
