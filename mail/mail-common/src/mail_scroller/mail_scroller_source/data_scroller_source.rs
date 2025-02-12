use anyhow::anyhow;
use proton_api_core::services::proton::common::LabelId;
use proton_core_common::{
    async_task::AsyncTaskResult,
    datatypes::LocalLabelId,
    models::{Label, ModelExtension},
};
use stash::stash::Tether;
use tracing::{debug, error, trace};

use crate::{
    datatypes::ReadFilter, mail_scroller::MailScrollerSet, models::CachedScrollData, AppError,
    MailContextError, MailUserContext,
};

use super::{remote_source::RemoteSource, MailPaginatorJoinHandle, MailScrollerSource};

#[derive(Debug)]
pub struct DataScrollerSource<T: RemoteSource> {
    local_label_id: LocalLabelId,
    unread: ReadFilter,
    page_size: usize,
    initialized: bool,
    ordered: Option<CachedScrollData<T>>,
    unordered: Option<CachedScrollData<T>>,
}

impl<T: RemoteSource> DataScrollerSource<T> {
    pub fn new(local_label_id: LocalLabelId, unread: ReadFilter, page_size: usize) -> Self {
        Self {
            local_label_id,
            unread,
            page_size,
            initialized: false,
            ordered: None,
            unordered: None,
        }
    }

    async fn sync_scroller(&mut self, tether: &Tether) -> Result<(), MailContextError> {
        if let Some(ref mut scroller) = self.ordered {
            if !scroller.has_more_than_a_page(tether).await? {
                scroller.update(tether).await?;
            }
        } else {
            self.ordered =
                CachedScrollData::new(self.local_label_id, self.unread, self.page_size, tether)
                    .await?;
        }

        Ok(())
    }

    async fn get_label(&self, tether: &Tether) -> Result<Label, MailContextError> {
        let Some(label) = Label::find_by_id(self.local_label_id, tether).await? else {
            return Err(AppError::LabelNotFound(self.local_label_id).into());
        };

        if label.remote_id.is_none() {
            return Err(AppError::LabelDoesNotHaveRemoteId(self.local_label_id).into());
        };

        Ok(label)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx, local_label_id, remote_label_id))]
    async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        T::sync_first_page(ctx, local_label_id, remote_label_id, unread, page_size).await
    }

    async fn spawn_background_sync(
        &self,
        ctx: &MailUserContext,
        scroller: &T,
        remote_label_id: LabelId,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let local_label_id = self.local_label_id;
        let unread = self.unread;
        let page_size = self.page_size;

        T::spawn_background_sync(
            ctx,
            local_label_id,
            scroller,
            remote_label_id,
            unread,
            page_size,
        )
        .await
    }
}

impl<T: RemoteSource> MailScrollerSource for DataScrollerSource<T> {
    type Item = T::Item;

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(u64, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let unread = self.unread;

        // Check if we have a data suggesting we have synced this label before
        if let Some(scroller) =
            CachedScrollData::new(self.local_label_id, self.unread, self.page_size, &tether).await?
        {
            debug!("We have paginated here before, create cached scroller");
            let task =
                if scroller.has_more_than_a_page(&tether).await? || total < self.page_size as u64 {
                    None
                } else {
                    let cp = scroller.scroll_data(&tether).await?;
                    self.spawn_background_sync(ctx, &cp, label.remote_id.clone().unwrap())
                        .await?
                };

            self.ordered = Some(scroller);

            return Ok((total, task));
        }

        // No entry exist, which means we have not synced this label yet.
        debug!("Paginating for the first time, getting first page & spawning sync task.");
        let local_label_id = label.local_id.unwrap();
        let remote_label_id = label.remote_id.clone().unwrap();
        let page_size = self.page_size;

        // Wait for the first page to be fetched
        let task = if total == 0 {
            None
        } else {
            Self::sync_first_page(ctx, local_label_id, remote_label_id, unread, page_size).await?
        };

        Ok((total, task))
    }

    async fn visible_items(
        &self,
        ctx: &MailUserContext,
    ) -> Result<Vec<Self::Item>, MailContextError> {
        // If cache is empty we have either
        // * an empty label
        // * a label that has not been initialized
        // The latter case is handled in the `Self::sync_more` method.
        // Here we simply assume empty label.
        if let Some(ref scroller) = self.unordered {
            let tether = ctx.user_stash().connection();
            Ok(scroller.visible_elements(&tether).await?)
        } else if !self.initialized {
            Ok(vec![])
        } else if let Some(ref scroller) = self.ordered {
            let tether = ctx.user_stash().connection();
            Ok(scroller.visible_elements(&tether).await?)
        } else {
            Ok(vec![])
        }
    }

    async fn visible_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        // If cache is empty we have either
        // * an empty label
        // * a label that has not been initialized
        // The latter case is handled in the `Self::sync_more` method.
        // Here we simply assume empty label.
        if let Some(ref scroller) = self.unordered {
            let tether = ctx.user_stash().connection();
            Ok(scroller.visible_element_count(&tether).await?)
        } else if !self.initialized {
            Ok(0)
        } else if let Some(ref scroller) = self.ordered {
            let tether = ctx.user_stash().connection();
            Ok(scroller.visible_element_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    async fn all_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();
        let total = T::total(self.local_label_id, self.unread, &tether).await?;

        Ok(total)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(MailScrollerSet<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let is_offline = ctx.session().status().await.is_offline();

        // Always sync the cache as there might be new data
        self.sync_scroller(&tether).await?;

        // We have ordered scroller which means we have at least first page in cache
        if let Some(ref mut scroller) = self.ordered {
            let items = if self.initialized {
                // This is the only place where cache progresses,
                // There might be a case in which someone will try to fetch more
                // for the label which has no more data.
                // The the cache will not progress and `items` will be empty.
                // Note: Task is always spawned, if there is no more data to download.
                // As this information is provided in a trait. It is up to the implementation
                // To check if there is more data to download before asking for more.
                MailScrollerSet::Append(scroller.fetch_more(&tether).await?)
            } else {
                // Never served any data, serve the first page
                MailScrollerSet::Replace(scroller.visible_elements(&tether).await?)
            };

            // If we switch between online and offline back and forth we need to make sure
            // to handle order of the items correctly to the user
            let items = match (is_offline, items.is_empty(), self.unordered.is_some()) {
                (true, true, true) => {
                    trace!("Offline, no more items in order, serve unordered items");
                    // Serve unordered items for timebeing
                    let items = self.unordered.as_mut().unwrap().fetch_more(&tether).await?;

                    MailScrollerSet::Append(items)
                }

                // We are pretty much offline without any more items in order, serve what more we have
                (true, true, false) => {
                    trace!("Offline, no more items in order, start serving unordered items");
                    // Clone cursor and replace the end-point with the unordered one
                    let mut scroller = scroller.clone().set_absolute_end();
                    let items = scroller.fetch_more(&tether).await?;
                    self.unordered = Some(scroller);

                    MailScrollerSet::Append(items)
                }

                // We are back online and have new items from API but also we have unordered items in the list, replace them
                (false, false, true) => {
                    trace!("Back online, serve ordered items");
                    self.unordered = None;

                    MailScrollerSet::Replace(scroller.visible_elements(&tether).await?)
                }

                // We are back online but we have no new ordered items. We may still have more unordered items to serve
                (false, true, true) => {
                    trace!(
                        "Back online, but previous request must have failed, serve unordered items"
                    );
                    let items = self.unordered.as_mut().unwrap().fetch_more(&tether).await?;

                    MailScrollerSet::Append(items)
                }

                // We are either online or offline but we may have more ordered items
                // from previous requests, if not there is no chance to recover - simply return the items
                _ => items,
            };

            let should_not_load_more_from_remote = scroller.has_more_than_a_page(&tether).await?
                || total < self.page_size as u64
                || is_offline;

            let task = if should_not_load_more_from_remote {
                None
            } else {
                let cp = scroller.scroll_data(&tether).await?;
                self.spawn_background_sync(ctx, &cp, label.remote_id.clone().unwrap())
                    .await?
            };

            self.initialized = true;

            Ok((items, total, task))

        // We have unordered scroller which means we never made any successful request
        // for that label and we have only unordered data in the cache.
        } else if self.unordered.is_some() {
            // Try to sync the label only when online
            // This check was put in place to not overload API with calls
            // when we are barely online.
            let task = if is_offline {
                None
            } else {
                let (_, task) = self.initialize(ctx).await?;
                task
            };

            let items = self.unordered.as_mut().unwrap().fetch_more(&tether).await?;

            Ok((MailScrollerSet::Append(items), total, task))

        // We've never synced the label - we have no scrollers.
        // This is first call ever and we are most likely offline.
        } else if total > 0 {
            trace!("We have never seen this label before, try to recover");
            // Try to get this first page once more.
            // This will not block the UI when we are offline.
            // The case when we are offline is handled by the MailScroller itself.
            let (_, task) = self.initialize(ctx).await?;

            if is_offline {
                debug!("We are offline, load whatever is in the cache");
                if let Some(scroller) =
                    CachedScrollData::all(self.local_label_id, self.unread, self.page_size, &tether)
                        .await?
                {
                    trace!("We have some data in the cache, serve it");
                    let items = scroller.visible_elements(&tether).await?;
                    self.unordered = Some(scroller);

                    // We fortunately have something to show, return it
                    return Ok((MailScrollerSet::Replace(items), total, task));
                }
            } else {
                // We have no data to show, but we are online, wait for the task to finish
                if let Some(task) = task {
                    task.await
                        .map_err(|_| {
                            MailContextError::Other(anyhow!("Failed to receive source data"))
                        })
                        .and_then(|res| match res {
                            AsyncTaskResult::Completed(v) => v,
                            AsyncTaskResult::Cancelled => Err(MailContextError::TaskCancelled),
                        })?;
                    if let Some(scroller) = CachedScrollData::new(
                        self.local_label_id,
                        self.unread,
                        self.page_size,
                        &tether,
                    )
                    .await?
                    {
                        let items = scroller.visible_elements(&tether).await?;
                        let cp = scroller.scroll_data(&tether).await?;
                        let task = self
                            .spawn_background_sync(ctx, &cp, label.remote_id.clone().unwrap())
                            .await?;

                        self.ordered = Some(scroller);
                        self.initialized = true;

                        // We've managed to get ordered data, return it
                        return Ok((MailScrollerSet::Replace(items), total, task));
                    }
                }
            }

            error!("Failed to serve any data for requested label");

            // We cannot do anything more, we have no data and/or we are offline.
            Err(MailContextError::no_connection())
        } else {
            // This is fallback for empty labels
            Ok((MailScrollerSet::Replace(vec![]), total, None))
        }
    }

    fn watched_tables(&self) -> Vec<String> {
        T::watched_tables()
    }
}
