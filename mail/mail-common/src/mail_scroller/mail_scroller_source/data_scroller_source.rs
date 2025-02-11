use proton_api_core::services::proton::common::LabelId;
use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, ModelExtension},
};
use stash::stash::Tether;
use tracing::debug;

use crate::{
    datatypes::ReadFilter, models::CachedScrollData, AppError, MailContextError, MailUserContext,
};

use super::{remote_source::RemoteSource, MailPaginatorJoinHandle, MailScrollerSource};

#[derive(Debug)]
pub struct DataScrollerSource<T: RemoteSource> {
    local_label_id: LocalLabelId,
    unread: ReadFilter,
    page_size: usize,
    initialized: bool,
    cached: Option<CachedScrollData<T>>,
}

impl<T: RemoteSource> DataScrollerSource<T> {
    pub fn new(local_label_id: LocalLabelId, unread: ReadFilter, page_size: usize) -> Self {
        Self {
            local_label_id,
            unread,
            page_size,
            initialized: false,
            cached: None,
        }
    }

    async fn sync_scroller(&mut self, tether: &Tether) -> Result<(), MailContextError> {
        if let Some(ref mut scroller) = self.cached {
            if !scroller.has_more_than_a_page(tether).await? {
                scroller.update(tether).await?;
            }
        } else {
            self.cached =
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

            self.cached = Some(scroller);

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
        let tether = ctx.user_stash().connection();

        // If cache is empty we have either
        // * an empty label
        // * a label that has not been initialized
        // The latter case is handled in the `Self::sync_more` method.
        // Here we simply assume empty label.
        if !self.initialized {
            Ok(vec![])
        } else if let Some(ref scroller) = self.cached {
            Ok(scroller.visible_elements(&tether).await?)
        } else {
            Ok(vec![])
        }
    }

    async fn visible_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();

        // If cache is empty we have either
        // * an empty label
        // * a label that has not been initialized
        // The latter case is handled in the `Self::sync_more` method.
        // Here we simply assume empty label.
        if !self.initialized {
            Ok(0)
        } else if let Some(ref scroller) = self.cached {
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
    ) -> Result<(Vec<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        // Always sync the cache as there might be new data
        self.sync_scroller(&tether).await?;

        if let Some(ref mut scroller) = self.cached {
            let items = if self.initialized {
                // This is the only place where cache progresses,
                // There might be a case in which someone will try to fetch more
                // for the label which has no more data.
                // The the cache will not progress and `items` will be empty.
                // Note: Task is always spawned, if there is no more data to download.
                // As this information is provided in a trait. It is up to the implementation
                // To check if there is more data to download before asking for more.
                scroller.fetch_more(&tether).await?
            } else {
                scroller.visible_elements(&tether).await?
            };

            let should_not_load_more_from_remote = scroller.has_more_than_a_page(&tether).await?
                || total < self.page_size as u64
                || ctx.session().status().await.is_offline();

            let task = if should_not_load_more_from_remote {
                None
            } else {
                let cp = scroller.scroll_data(&tether).await?;
                self.spawn_background_sync(ctx, &cp, label.remote_id.clone().unwrap())
                    .await?
            };

            self.initialized = true;
            Ok((items, total, task))
        } else if total > 0 {
            if ctx.session().status().await.is_offline() {
                return Err(MailContextError::no_connection());
            }

            // Fallback for failing to initialize the scroller
            let (_, task) = self.initialize(ctx).await?;
            Ok((vec![], total, task))
        } else {
            // This is fallback for empty labels
            Ok((vec![], total, None))
        }
    }

    fn watched_tables(&self) -> Vec<String> {
        T::watched_tables()
    }
}
