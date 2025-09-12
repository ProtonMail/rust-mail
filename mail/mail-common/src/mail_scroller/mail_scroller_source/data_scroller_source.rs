use anyhow::anyhow;
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, ModelExtension},
};
use stash::orm::Model;
use stash::stash::Tether;
use tracing::{debug, warn};

use super::{
    MailPaginatorJoinHandle, MailScrollerSource, mail_scroller_state::MailScrollerState,
    remote_source::RemoteSource,
};
use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::{AppError, MailContextError, MailUserContext, datatypes::ReadFilter};

#[derive(Debug)]
pub struct DataScrollerSource<T: RemoteSource> {
    local_label_id: LocalLabelId,
    unread: ReadFilter,
    page_size: usize,
    invalidate: Option<flume::Sender<()>>,
    new_data_callback: (flume::Sender<()>, flume::Receiver<()>),
    order_dir: ScrollOrderDir,
    order_field: ScrollOrderField,
    state: MailScrollerState<T>,
}

impl<T: RemoteSource> DataScrollerSource<T> {
    pub fn new(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Self {
        Self {
            local_label_id,
            unread,
            page_size,
            invalidate: None,
            new_data_callback: flume::bounded(1),
            state: MailScrollerState::new_not_synced(
                local_label_id,
                unread,
                page_size,
                order_dir,
                order_field,
            ),
            order_dir,
            order_field,
        }
    }

    async fn initialize_impl(
        &mut self,
        ctx: &MailUserContext,
        check_for_total: bool,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        tracing::info!("Initializing MailScroller Source");
        let mut tether = ctx.user_stash().connection().await?;
        let label = self.get_label(&tether).await?;
        let remote_label_id = label.remote_id.clone().unwrap();
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let unread = self.unread;
        let is_offline = ctx.network_monitor_service().is_os_offline();
        let is_online = !is_offline;

        self.sync_scroller(&tether).await?;

        // Check if we have a data suggesting we have synced this label before
        if let Some(scroller) = self.state.online() {
            debug!(
                "We have paginated here before, try to sync data, status: {}",
                if is_online { "online" } else { "offline" }
            );
            if let Some(scroll_data) = scroller.scroll_data_begin(&tether).await? {
                debug!("Syncing previous page in background");

                self.sync_previous_page(ctx, &scroll_data, remote_label_id.clone())
                    .await?;
                let task = if is_online
                    && !scroller.has_next_page(&tether).await?
                    && total > self.page_size as u64
                {
                    debug!("Syncing next page in a task");
                    self.sync_next_page(ctx, &scroll_data, remote_label_id)
                        .await?
                } else {
                    None
                };
                return Ok(task);
            } else {
                debug!("Cursor points to empty scroll data, will sync first page instead");
                let scroll_data = scroller.scroll_data_end(&tether).await?;
                tether
                    .tx(async |bond| scroll_data.delete(bond).await)
                    .await?;
            };
        }

        // No entry exist, which means we have not synced this label yet.
        debug!(
            "Paginating for the first time, getting first page while being {}.",
            if is_offline { "offline" } else { "online" }
        );

        // Clear the state if we had cursor pointing to empty scroll data
        if self.state.is_online() {
            self.clear_state();
        }

        let local_label_id = label.id();
        let remote_label_id = label.remote_id.clone().unwrap();

        let task = if check_for_total && total == 0 {
            None
        } else {
            Self::sync_first_page(
                ctx,
                local_label_id,
                remote_label_id,
                unread,
                self.page_size,
                self.order_dir,
                self.order_field,
            )
            .await?
        };

        Ok(task)
    }

    /// Send a message to notify user that the scrolling data order have chagned.
    async fn notify_scroller_order_invalid(
        invalidate: &Option<flume::Sender<()>>,
    ) -> Result<(), MailContextError> {
        if let Some(sender) = invalidate.as_ref() {
            sender.send_async(()).await.map_err(|e| {
                MailContextError::Other(anyhow!(
                    "Could not notify about invalid scroller state: {e}"
                ))
            })?;
        }

        Ok(())
    }

    /// Update ordered scroller end cursor to the newest value.
    ///
    /// Method will set it to Online if there is data to show
    /// Otherwise it will leave state unmodified.
    async fn sync_scroller(&mut self, tether: &Tether) -> Result<(), MailContextError> {
        let old_state = self.state.to_string();
        self.state
            .sync(self.local_label_id, self.unread, self.page_size, tether)
            .await?;

        debug!(
            "Sync scroller state, old: {}, new: {}",
            old_state, self.state
        );

        Ok(())
    }

    /// Get label from the database for which scroller is created.
    async fn get_label(&self, tether: &Tether) -> Result<Label, MailContextError> {
        let Some(label) = Label::find_by_id(self.local_label_id, tether).await? else {
            return Err(AppError::LabelNotFound(self.local_label_id).into());
        };

        if label.remote_id.is_none() {
            return Err(AppError::LabelDoesNotHaveRemoteId(self.local_label_id).into());
        };

        Ok(label)
    }

    #[tracing::instrument(skip_all, fields(label_id=local_label_id.as_u64(), unread=?unread) )]
    async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        T::sync_first_page(
            ctx,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
            order_dir,
            order_field,
        )
        .await
    }

    async fn sync_next_page(
        &self,
        ctx: &MailUserContext,
        scroller: &T,
        remote_label_id: LabelId,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let local_label_id = self.local_label_id;
        let unread = self.unread;
        let page_size = self.page_size;

        T::sync_next_page(
            ctx,
            local_label_id,
            scroller,
            remote_label_id,
            unread,
            page_size,
            self.order_dir,
            self.order_field,
        )
        .await
    }

    async fn sync_previous_page(
        &self,
        ctx: &MailUserContext,
        scroller: &T,
        remote_label_id: LabelId,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let local_label_id = self.local_label_id;
        let unread = self.unread;
        let page_size = self.page_size;
        let task = T::sync_previous_page(
            ctx,
            local_label_id,
            scroller,
            remote_label_id,
            unread,
            page_size,
            self.order_dir,
            self.order_field,
            self.new_data_callback.0.clone(),
        )
        .await?;

        Ok(task)
    }

    fn clear_state(&mut self) {
        self.state = MailScrollerState::new_not_synced(
            self.local_label_id,
            self.unread,
            self.page_size,
            self.order_dir,
            self.order_field,
        );
    }
}

impl<T: RemoteSource> MailScrollerSource for DataScrollerSource<T> {
    type Item = T::Item;

    #[tracing::instrument(skip_all, fields(label_id=self.local_label_id.as_u64(), unread=?self.unread) )]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        self.initialize_impl(ctx, false).await
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
        if let Some(scroller) = self.state.not_synced() {
            let tether = ctx.user_stash().connection().await?;
            Ok(scroller.visible_elements(&tether).await?)
        } else if let Some(scroller) = self.state.online() {
            let tether = ctx.user_stash().connection().await?;
            Ok(scroller.visible_elements(&tether).await?)
        } else {
            Ok(vec![])
        }
    }

    async fn seen_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        // If cache is empty we have either
        // * an empty label
        // * a label that has not been initialized
        // The latter case is handled in the `Self::sync_more` method.
        // Here we simply assume empty label.
        if let Some(scroller) = self.state.not_synced() {
            let tether = ctx.user_stash().connection().await?;
            Ok(scroller.seen_count(&tether).await?)
        } else if let Some(scroller) = self.state.online() {
            let tether = ctx.user_stash().connection().await?;
            Ok(scroller.seen_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    async fn synced_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        if let Some(scroller) = self.state.not_synced() {
            let tether = ctx.user_stash().connection().await?;
            Ok(scroller.synced_count(&tether).await?)
        } else if let Some(scroller) = self.state.online() {
            let tether = ctx.user_stash().connection().await?;
            Ok(scroller.synced_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    async fn all_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;

        Ok(total)
    }

    async fn has_more(&self, ctx: &MailUserContext) -> Result<bool, MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let has_more = self.state.has_more_in_order(&tether).await?;

        Ok(has_more)
    }

    #[tracing::instrument(skip_all, fields(label_id=self.local_label_id.as_u64(), unread=?self.unread) )]
    async fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(Vec<Self::Item>, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let label = self.get_label(&tether).await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let is_offline = ctx.network_monitor_service().is_os_offline();
        let is_online = !is_offline;

        // If we have loaded previous page in background, we need to replace
        let new_data_arrived = self.new_data_callback.1.try_recv().is_ok();
        let mut replace = new_data_arrived;

        // Always sync the cache as there might be new data.
        // The sync has to be done after determining the previous
        // state. This is the soonest place it can be safely called.
        self.sync_scroller(&tether).await?;

        if is_online
            && self.state.is_not_synced()
            && let Some(task) = self.initialize_impl(ctx, true).await?
        {
            match task.await {
                Ok(Ok(_)) => (),
                Ok(Err(err)) => {
                    warn!(?err, "Couldn't initialize scroller, continuing anyway");
                }
                Err(err) => {
                    warn!(?err, "Couldn't initialize scroller, continuing anyway");
                }
            }

            self.sync_scroller(&tether).await?;
            replace = true;
        }

        let (items, task) = match &mut self.state {
            MailScrollerState::Online(scroller) => {
                // This is the only place where cache progresses,
                // There might be a case in which someone will try to fetch more
                // for the label which has no more data.
                // The the cache will not progress and `items` will be empty.
                // Note: Task is always spawned, if there is no more data to download.
                // As this information is provided in a trait. It is up to the implementation
                // To check if there is more data to download before asking for more.
                let items = scroller.fetch_more(&tether).await?;
                let items = if replace {
                    debug!(
                        "Items displayed on the screen are unordered, new_data: {} notifying client to reload",
                        new_data_arrived
                    );
                    Self::notify_scroller_order_invalid(&self.invalidate).await?;
                    vec![]
                } else {
                    items
                };

                let has_next_page = scroller.has_next_page(&tether).await?;
                let is_small_label = total > 0 && total < self.page_size as u64;

                let should_load_more_from_remote = !has_next_page || is_small_label;

                debug!(
                    "Should load more from remote: {}, is small label: {}, has next page: {}, is online: {}",
                    should_load_more_from_remote, is_small_label, has_next_page, is_online
                );

                let task = if should_load_more_from_remote {
                    let cp = scroller.scroll_data_end(&tether).await?;
                    self.sync_next_page(ctx, &cp, label.remote_id.clone().unwrap())
                        .await?
                } else {
                    None
                };

                (items, task)
            }
            MailScrollerState::NotSynced(unordered) => (unordered.fetch_more(&tether).await?, None),
        };

        Ok((items, task))
    }

    /// TODO: Try to merge it with initialize past 0.142.xyz release
    #[tracing::instrument(skip_all, fields(label_id=self.local_label_id.as_u64(), unread=?self.unread) )]
    async fn sync_new(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        tracing::info!("Syncing newest items");
        let tether = ctx.user_stash().connection().await?;
        let label = self.get_label(&tether).await?;
        let remote_label_id = label.remote_id.clone().unwrap();
        let unread = self.unread;
        let is_offline = ctx.network_monitor_service().is_os_offline();
        let is_online = !is_offline;

        self.sync_scroller(&tether).await?;

        // Check if we have a data suggesting we have synced this label before
        if let Some(scroller) = self.state.online() {
            debug!(
                "We have paginated here before, try to sync data, status: {}",
                if is_online { "online" } else { "offline" }
            );
            if let Some(scroll_data) = scroller.scroll_data_begin(&tether).await? {
                debug!("Syncing previous page in a task");

                let task = self
                    .sync_previous_page(ctx, &scroll_data, remote_label_id.clone())
                    .await?;
                let task = if is_online { task } else { None };

                return Ok(task);
            } else {
                debug!("Cursor points to empty scroll data, will sync first page instead");
            };
        }

        // No entry exist, which means we have not synced this label yet.
        debug!(
            "Paginating for the first time, getting first page while being {}.",
            if is_offline { "offline" } else { "online" }
        );

        let local_label_id = label.id();
        let remote_label_id = label.remote_id.clone().unwrap();
        let task = Self::sync_first_page(
            ctx,
            local_label_id,
            remote_label_id,
            unread,
            self.page_size,
            self.order_dir,
            self.order_field,
        )
        .await?;

        Ok(task)
    }

    async fn change_filter(
        &mut self,
        ctx: &MailUserContext,
        filter: ReadFilter,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        self.unread = filter;
        self.state =
            MailScrollerState::new_online(self.local_label_id, filter, self.page_size, &tether)
                .await?;
        debug!("Changed filter, new state: {}, initializing...", self.state);

        let task = self.initialize(ctx).await?;

        Ok(task)
    }

    async fn clear_cursor(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        if let Some(scroller) = self.state.online() {
            tracing::info!("Clearing cache for label {}", self.local_label_id);
            let mut tether = ctx.user_stash().connection().await?;
            let cursor = scroller.scroll_data_end(&tether).await?;
            tether.tx(async |tx| cursor.delete(tx).await).await?;
        }
        self.clear_state();
        let task = self.initialize(ctx).await?;

        Ok(task)
    }

    fn watched_tables(&self) -> Vec<String> {
        T::watched_tables()
    }

    fn set_notify(&mut self, sender: flume::Sender<()>) {
        let _ = self.invalidate.insert(sender);
    }
}
