use anyhow::anyhow;
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, ModelExtension},
};
use stash::orm::Model;
use stash::stash::Tether;
use tracing::{debug, trace};

use super::{
    MailPaginatorJoinHandle, MailScrollerSource, mail_scroller_state::MailScrollerState,
    remote_source::RemoteSource,
};
use crate::datatypes::labels::LabelScrollOrder;
use crate::{AppError, MailContextError, MailUserContext, datatypes::ReadFilter};

#[derive(Debug)]
pub struct DataScrollerSource<T: RemoteSource> {
    local_label_id: LocalLabelId,
    unread: ReadFilter,
    page_size: usize,
    invalidate: Option<flume::Sender<()>>,
    new_data_callback: (flume::Sender<()>, flume::Receiver<()>),
    scroll_order: LabelScrollOrder,
    state: MailScrollerState<T>,
}

impl<T: RemoteSource> DataScrollerSource<T> {
    pub fn new(
        local_label_id: LocalLabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Self {
        Self {
            local_label_id,
            unread,
            page_size,
            invalidate: None,
            new_data_callback: flume::bounded(0),
            state: MailScrollerState::None,
            scroll_order,
        }
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
        self.state
            .sync(self.local_label_id, self.unread, self.page_size, tether)
            .await?;

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

    /// Wrapper function to recover on MailScrollerState::None to handle the case when we have never visited the label.
    async fn offline_fallback(
        &mut self,
        ctx: &MailUserContext,
        tether: &Tether,
    ) -> Result<(Vec<T::Item>, MailPaginatorJoinHandle), MailContextError> {
        let scroll_order =
            LabelScrollOrder::for_local_label_id(self.local_label_id, tether).await?;
        self.state = MailScrollerState::new_not_synced(
            self.local_label_id,
            self.unread,
            self.page_size,
            scroll_order,
        );

        debug!("We are offline, load whatever is in the cache");
        let items = self.state.offline_mut().unwrap().fetch_more(tether).await?;
        // Low chance to get a task back, but lets try anyway
        let (_, task) = self.initialize(ctx).await?;

        Ok((items, task))
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx, local_label_id, remote_label_id))]
    async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        T::sync_first_page(
            ctx,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
            scroll_order,
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
            self.scroll_order,
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
            self.scroll_order,
            self.new_data_callback.0.clone(),
        )
        .await?;

        Ok(task)
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
        let remote_label_id = label.remote_id.clone().unwrap();
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let unread = self.unread;
        // On the initialization of the scroller its vital to be able to
        // recognize the connection state with high precision due to the fact
        // it may impare how the whole application is behaving.
        //
        // double call is done in order to utilize low_latency check
        // only when we may be online but we have to confirm it, never for offline
        // as we will have a 2 sec downtime everytime we load location in offline.
        let is_offline = ctx.session().status().await.is_offline();
        let is_offline = is_offline
            || ctx
                .session()
                .status_watcher()
                .low_latency_status(ctx.api().clone())
                .await
                .is_offline();

        self.sync_scroller(&tether).await?;

        if is_offline {
            debug!("We are offline, return scroller without a task");

            return Ok((total, None));
        }

        // Check if we have a data suggesting we have synced this label before
        if let Some(scroller) = self.state.online() {
            debug!("We have paginated here before, create cached scroller");
            let task = match scroller.scroll_data_begin(&tether).await? {
                Some(scroll_data) => {
                    if !scroller.has_more_than_a_page(&tether).await?
                        && total > self.page_size as u64
                    {
                        debug!("Syncing next page in a task");
                        self.sync_next_page(ctx, &scroll_data, remote_label_id)
                            .await?
                    } else {
                        debug!("Syncing previous page in background");
                        self.sync_previous_page(ctx, &scroll_data, remote_label_id)
                            .await?;

                        // Previous page should not be awaited
                        None
                    }
                }
                // When someone decides to oblitarate its mails it may happen that we think we have data in order
                // but in reality the cursor cant get anything and this can lead to undefined behaviors.
                // So lets make sure we have first page at least.
                None => {
                    Self::sync_first_page(
                        ctx,
                        self.local_label_id,
                        remote_label_id,
                        unread,
                        self.page_size,
                        self.scroll_order,
                    )
                    .await?
                }
            };

            return Ok((total, task));
        }

        // No entry exist, which means we have not synced this label yet.
        debug!("Paginating for the first time, getting first page");
        let local_label_id = label.id();
        let remote_label_id = label.remote_id.clone().unwrap();
        let page_size = self.page_size;

        let task = if total == 0 {
            None
        } else {
            Self::sync_first_page(
                ctx,
                local_label_id,
                remote_label_id,
                unread,
                page_size,
                self.scroll_order,
            )
            .await?
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
        if let Some(scroller) = self.state.offline() {
            let tether = ctx.user_stash().connection();
            Ok(scroller.visible_elements(&tether).await?)
        } else if let Some(scroller) = self.state.online() {
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
        if let Some(scroller) = self.state.offline() {
            let tether = ctx.user_stash().connection();
            Ok(scroller.visible_element_count(&tether).await?)
        } else if let Some(scroller) = self.state.online() {
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
    ) -> Result<(Vec<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;
        let total = T::total(self.local_label_id, self.unread, &tether).await?;
        let is_offline = ctx.session().status().await.is_offline();
        let is_online = !is_offline;

        // If we go back online, we need to replace
        // or when we have loaded previous page in bg
        let new_data_arrived = self.new_data_callback.1.try_recv().is_ok();
        let connection_returned = self.state.is_offline() && is_online;
        let replace = connection_returned || new_data_arrived;

        // Set state accordingly to the current connection status
        if is_offline && !self.state.has_more_in_order(&tether).await? {
            self.state.to_offline();
        } else if self.state.is_not_synced() && is_online {
            debug!("Mail Scroller was never initialized, finishing initialization");
            let (_, task) = self.initialize(ctx).await?;

            if let Some(task) = task {
                // This has to be awaiten here, we have no way to establish
                // switch from NonSynced -> Online if initialize do not spawn
                // task when offline
                task.await?;
            }
        } else {
            self.state.to_online();
        };

        // Always sync the cache as there might be new data.
        // The sync has to be done after determining the previous
        // state. This is the soonest place it can be safely called.
        self.sync_scroller(&tether).await?;

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
                        "Items displayed on the screen are unordered, notifing client to reload"
                    );
                    Self::notify_scroller_order_invalid(&self.invalidate).await?;

                    vec![]
                } else {
                    items
                };

                let should_not_load_more_from_remote =
                    scroller.has_more_than_a_page(&tether).await?
                        || total < self.page_size as u64
                        || is_offline;

                let task = if should_not_load_more_from_remote {
                    None
                } else {
                    let cp = scroller.scroll_data_end(&tether).await?;
                    self.sync_next_page(ctx, &cp, label.remote_id.clone().unwrap())
                        .await?
                };

                (items, task)
            }
            MailScrollerState::Offline {
                ordered: _,
                unordered,
            }
            | MailScrollerState::NotSynced(unordered) => {
                (unordered.fetch_more(&tether).await?, None)
            }

            MailScrollerState::None => (vec![], None),
        };

        // When Scroller was never initialized try to figure out if we can serve something
        // offline_fallback code will be executed more than once only if we are online
        // but we never managed to download any data for the label with total > 0
        // This is extreme case but will force user to wait for the data to be downloaded
        let (items, task) = if self.state.is_none() && total > 0 {
            trace!("We have never seen this label before, try to recover");
            self.offline_fallback(ctx, &tether).await?
        } else {
            (items, task)
        };

        Ok((items, total, task))
    }

    fn watched_tables(&self) -> Vec<String> {
        T::watched_tables()
    }

    fn set_notify(&mut self, sender: flume::Sender<()>) {
        let _ = self.invalidate.insert(sender);
    }

    async fn invalidate(&mut self) -> Result<(), MailContextError> {
        Self::notify_scroller_order_invalid(&self.invalidate).await?;

        Ok(())
    }
}
