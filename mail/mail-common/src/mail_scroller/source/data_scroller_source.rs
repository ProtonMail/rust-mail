use super::{
    MailPaginatorJoinHandle, MailScrollerSource, mail_scroller_state::MailScrollerState,
    remote_source::RemoteSource,
};
use crate::datatypes::SearchOptions;
use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::{AppError, MailContextError, MailUserContext, datatypes::ReadFilter};
use anyhow::anyhow;
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, ModelExtension},
};
use stash::stash::Tether;
use tracing::{debug, info, instrument, warn};

#[derive(Debug)]
pub struct DataScrollerSource<T: RemoteSource> {
    local_label_id: LocalLabelId,
    unread: ReadFilter,
    page_size: usize,
    invalidate: Option<flume::Sender<()>>,
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
            state: MailScrollerState::unsynced(
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

    #[instrument(skip_all)]
    async fn initialize_impl(
        &mut self,
        ctx: &MailUserContext,
        check_for_total: bool,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        info!("Initializing MailScroller Source");

        let mut tether = ctx.user_stash().connection().await?;
        let label = self.get_label(&tether).await?;
        let remote_label_id = label.remote_id.clone().unwrap();
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let is_offline = ctx.network_monitor_service().is_os_offline();
        let is_online = !is_offline;

        self.sync_scroller(&tether).await?;

        if let Some(scroller) = self.state.as_synced() {
            debug!(
                "We have paginated here before, try to sync data, status: {}",
                if is_online { "online" } else { "offline" }
            );

            let end_cursor = scroller.load_end_cursor(&tether).await?;

            if let Some(ending_element) = scroller.scroll_data_end(&tether).await?
                && let Some(beggining_element) = scroller.scroll_data_begin(&tether).await?
                && T::item_id(&ending_element) == T::item_id(&end_cursor)
            {
                debug!("Syncing previous page in background");

                self.sync_previous_page(
                    ctx,
                    &beggining_element,
                    remote_label_id.clone(),
                    self.invalidate.clone(),
                )?;

                let task = if is_online
                    && !scroller.has_next_page(&tether).await?
                    && total > self.page_size as u64
                {
                    debug!("Syncing next page in a task");

                    self.sync_next_page(ctx, &ending_element, remote_label_id)?
                } else {
                    None
                };

                return Ok(task);
            } else {
                debug!("Cursor points to empty scroll data, will sync first page instead");

                tether
                    .tx(async |bond| end_cursor.delete(bond).await)
                    .await?;
            };
        }

        debug!(
            "Paginating for the first time, getting first page while being {}.",
            if is_offline { "offline" } else { "online" }
        );

        if self.state.is_synced() {
            self.clear_state();
        }

        let has_more = self.state.has_more(&tether).await?;
        let remote_label_id = label.remote_id.clone().unwrap();

        let task = if check_for_total && total == 0 {
            None
        } else if has_more {
            debug!("We have local data, running first page sync in background");

            self.sync_first_page(
                ctx,
                remote_label_id,
                self.order_dir,
                self.order_field,
                self.invalidate.clone(),
            )?;

            None
        } else {
            debug!("We have no local data, running first page sync in a task");

            self.sync_first_page(ctx, remote_label_id, self.order_dir, self.order_field, None)?
        };

        Ok(task)
    }

    async fn notify_invalidated(
        invalidate: &Option<flume::Sender<()>>,
    ) -> Result<(), MailContextError> {
        if let Some(sender) = invalidate.as_ref() {
            sender.send_async(()).await.map_err(|e| {
                MailContextError::Other(anyhow!(
                    "Could not notify about invalidated scroller state: {e}"
                ))
            })?;
        }

        Ok(())
    }

    #[instrument(skip_all)]
    async fn sync_scroller(&mut self, tether: &Tether) -> Result<(), MailContextError> {
        let old_state = self.state.to_string();

        self.state
            .sync(self.local_label_id, self.unread, self.page_size, tether)
            .await?;

        let new_state = self.state.to_string();

        if old_state != new_state {
            debug!(
                "Changing scroller's state from {} to {}",
                old_state, new_state,
            );
        }

        Ok(())
    }

    #[instrument(skip_all)]
    async fn get_label(&self, tether: &Tether) -> Result<Label, MailContextError> {
        let Some(label) = Label::find_by_id(self.local_label_id, tether).await? else {
            return Err(AppError::LabelNotFound(self.local_label_id).into());
        };

        if label.remote_id.is_none() {
            return Err(AppError::LabelDoesNotHaveRemoteId(self.local_label_id).into());
        };

        Ok(label)
    }

    #[instrument(skip_all)]
    fn sync_first_page(
        &self,
        ctx: &MailUserContext,
        remote_label_id: LabelId,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        invalidate: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        debug!(
            ?remote_label_id,
            ?order_dir,
            ?order_field,
            "Syncing first page (async)"
        );

        let local_label_id = self.local_label_id;
        let unread = self.unread;
        let page_size = self.page_size;

        T::sync_first_page(
            ctx,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
            order_dir,
            order_field,
            invalidate,
        )
    }

    #[instrument(skip_all)]
    fn sync_next_page(
        &self,
        ctx: &MailUserContext,
        scroller: &T,
        remote_label_id: LabelId,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        debug!("Syncing next page (async)");

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
    }

    #[instrument(skip_all)]
    fn sync_previous_page(
        &self,
        ctx: &MailUserContext,
        scroller: &T,
        remote_label_id: LabelId,
        invalidate: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        debug!("Syncing previous page (async)");

        let local_label_id = self.local_label_id;
        let unread = self.unread;
        let page_size = self.page_size;

        T::sync_previous_page(
            ctx,
            local_label_id,
            scroller,
            remote_label_id,
            unread,
            page_size,
            self.order_dir,
            self.order_field,
            invalidate,
        )
    }

    #[instrument(skip_all)]
    fn clear_state(&mut self) {
        debug!("Clearing state");

        self.state = MailScrollerState::unsynced(
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

    #[instrument(skip_all, fields(label_id=self.local_label_id.as_u64(), unread=?self.unread) )]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
        invalidate: flume::Sender<()>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        self.invalidate = Some(invalidate);
        self.initialize_impl(ctx, false).await
    }

    async fn visible_elements(
        &self,
        ctx: &MailUserContext,
    ) -> Result<Vec<Self::Item>, MailContextError> {
        let tether = ctx.user_stash().connection().await?;

        match &self.state {
            MailScrollerState::Synced(state) => Ok(state.visible_elements(&tether).await?),
            MailScrollerState::Unsynced(state) => Ok(state.visible_elements(&tether).await?),
        }
    }

    async fn seen_count(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection().await?;

        match &self.state {
            MailScrollerState::Synced(state) => Ok(state.seen_count(&tether).await?),
            MailScrollerState::Unsynced(state) => Ok(state.seen_count(&tether).await?),
        }
    }

    async fn synced_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection().await?;

        match &self.state {
            MailScrollerState::Synced(state) => Ok(state.synced_count(&tether).await?),
            MailScrollerState::Unsynced(state) => Ok(state.synced_count(&tether).await?),
        }
    }

    async fn all_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;

        Ok(total)
    }

    async fn has_more(&self, ctx: &MailUserContext) -> Result<bool, MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let has_more = self.state.has_more_synced(&tether).await?;

        Ok(has_more)
    }

    #[instrument(skip_all, fields(label_id=self.local_label_id.as_u64(), unread=?self.unread) )]
    async fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(Vec<Self::Item>, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let label = self.get_label(&tether).await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let is_offline = ctx.network_monitor_service().is_os_offline();
        let is_online = !is_offline;

        // If we have loaded first or previous page in background
        // and we have seen some data already, we need to replace
        let mut replace = self.state.is_unsynced() && self.state.seen_count(&tether).await? > 0;

        // Always sync the cache as there might be new data.
        // The sync has to be done after determining the previous
        // state. This is the soonest place it can be safely called.
        self.sync_scroller(&tether).await?;

        // If we never synced this label before due to network issues now is the time to do it.
        if is_online
            && self.state.is_unsynced()
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
            MailScrollerState::Synced(scroller) => {
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
                        "Items displayed on the screen are not synced, notifying client to reload"
                    );
                    Self::notify_invalidated(&self.invalidate).await?;
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
                    let cp = scroller.load_end_cursor(&tether).await?;

                    self.sync_next_page(ctx, &cp, label.remote_id.clone().unwrap())?
                } else {
                    None
                };

                (items, task)
            }
            MailScrollerState::Unsynced(unordered) => (unordered.fetch_more(&tether).await?, None),
        };

        Ok((items, task))
    }

    #[instrument(skip_all, fields(label_id=self.local_label_id.as_u64(), unread=?self.unread) )]
    async fn sync_new(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        info!("Syncing newest items");

        let tether = ctx.user_stash().connection().await?;
        let label = self.get_label(&tether).await?;
        let remote_label_id = label.remote_id.clone().unwrap();
        let is_offline = ctx.network_monitor_service().is_os_offline();
        let is_online = !is_offline;

        self.sync_scroller(&tether).await?;

        if let Some(scroller) = self.state.as_synced() {
            debug!(
                "We have paginated here before, try to sync data, status: {}",
                if is_online { "online" } else { "offline" }
            );

            if let Some(scroll_data) = scroller.scroll_data_begin(&tether).await? {
                debug!("Syncing previous page in background");

                self.sync_previous_page(
                    ctx,
                    &scroll_data,
                    remote_label_id.clone(),
                    self.invalidate.clone(),
                )?;

                return Ok(None);
            } else {
                debug!("Cursor points to empty scroll data, will sync first page instead");
            };
        }

        debug!(
            "Paginating for the first time, getting first page while being {}.",
            if is_offline { "offline" } else { "online" }
        );

        let remote_label_id = label.remote_id.clone().unwrap();

        self.sync_first_page(ctx, remote_label_id, self.order_dir, self.order_field, None)
    }

    async fn change_state(
        &mut self,
        ctx: &MailUserContext,
        unread: Option<ReadFilter>,
        label: Option<LocalLabelId>,
        _keywords: Option<SearchOptions>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let tether = ctx.user_stash().connection().await?;

        if let Some(unread) = unread {
            info!(
                "Changing unread filter from {current:?} to {unread:?}",
                current = self.unread,
            );
            self.unread = unread;
        }

        if let Some(label) = label {
            info!(
                "Changing label from {current} to {label}",
                current = self.local_label_id
            );
            self.local_label_id = label;
        }

        self.state =
            MailScrollerState::synced(self.local_label_id, self.unread, self.page_size, &tether)
                .await?;

        debug!("Changed state, new state: {}, initializing...", self.state);

        let task = self.initialize_impl(ctx, false).await?;

        Ok(task)
    }

    async fn clear(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        if let Some(scroller) = self.state.as_synced() {
            info!("Clearing cache for label {}", self.local_label_id);

            let mut tether = ctx.user_stash().connection().await?;
            let cursor = scroller.load_end_cursor(&tether).await?;

            tether.tx(async |tx| cursor.delete(tx).await).await?;
        }

        self.clear_state();

        let task = self.initialize_impl(ctx, false).await?;

        Ok(task)
    }

    fn watched_tables(&self) -> Vec<String> {
        T::watched_tables()
    }
}
