mod mail_scroller_source;
mod mail_scroller_watcher;

pub use self::mail_scroller_source::*;
pub use self::mail_scroller_watcher::*;
use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::datatypes::{
    AlmostAllMail, ContextualConversation, IncludeSwitch, ReadFilter, SearchOptions, SystemLabelId,
};
use crate::models::MailSettings;
use crate::models::{ConversationScrollData, Message, MessageScrollData};
use crate::traits::ScrollerEq;
use crate::{MailContextError, MailUserContext};
use anyhow::anyhow;
use derive_more::Display;
use futures::select;
use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::app_events::{OnEnterForegroundEvent, OnForceEventPollEvent};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelIdExtension};
use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use stash::orm::Model;
use stash::stash::{Tether, WatcherHandle};
use std::sync::{Arc, Weak};
use tokio::sync::{RwLock, oneshot};
use tokio::task::AbortHandle;
use uuid::Uuid;

#[cfg(test)]
#[path = "tests/mail_scroller/message_scroller.rs"]
mod message_scroller;

#[cfg(test)]
#[path = "tests/mail_scroller/conversation_scroller.rs"]
mod conversation_scroller;

#[derive(Debug, thiserror::Error)]
pub enum MailScrollerError {
    #[error("MailScroller cannot serve more data, counters seems not to be fulfillable")]
    NotSynced,
}

#[derive(Debug)]
pub enum ScrollerUpdate<T> {
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
    ReplaceRange {
        src: ScrollerSource,
        from: usize,
        to: usize,
        items: Vec<T>,
    },
    Error {
        src: ScrollerSource,
        error: MailContextError,
    },
}

impl<T> ScrollerUpdate<T> {
    pub fn is_none(&self) -> bool {
        matches!(self, ScrollerUpdate::None(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, ScrollerUpdate::Error { .. })
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
            ScrollerUpdate::ReplaceRange { src, .. } => src,
            ScrollerUpdate::Error { src, .. } => src,
        }
    }

    pub fn is_scroll_event(&self) -> bool {
        matches!(self.src(), ScrollerSource::ScrollEvent(_))
    }
}

#[derive(Copy, Clone, Debug, Display)]
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
    id: Uuid,
    command: flume::Sender<ScrollerCommand>,
    ordered_command: flume::Sender<ScrollerOrderedCommand>,
    supports_include_filter: bool,
    aborts: Vec<AbortHandle>,
}

impl Drop for MailScroller {
    fn drop(&mut self) {
        tracing::debug!(
            ?self.id,
            "Dropping MailScroller, aborting {} tasks",
            self.aborts.len()
        );

        self.terminate()
    }
}

pub struct MailScrollerHandle<T> {
    pub updates: flume::Receiver<ScrollerUpdate<T>>,
    pub handle: DropRemoveTableObserverHandle,
}

impl MailScroller {
    pub async fn conversations(
        ctx: Weak<MailUserContext>,
        mut label: LocalLabelId,
        unread: ReadFilter,
        include: IncludeSwitch,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<ContextualConversation>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection().await?;

        let alt_label = Self::alternative_label(label, &tether).await?;
        let order_dir = ScrollOrderDir::for_local_label(label, &tether).await?;
        let order_field = ScrollOrderField::for_local_label(label, &tether).await?;

        if include.has_spam_and_trash()
            && let Some(alt_label) = alt_label
        {
            label = alt_label;
        }

        let source = DataScrollerSource::<ConversationScrollData>::new(
            label,
            alt_label,
            unread,
            page_size,
            order_dir,
            order_field,
        );

        Self::new(ctx, source, page_size, alt_label.is_some()).await
    }

    pub async fn messages(
        ctx: Weak<MailUserContext>,
        mut label: LocalLabelId,
        unread: ReadFilter,
        include: IncludeSwitch,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<Message>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection().await?;

        let alt_label = Self::alternative_label(label, &tether).await?;
        let order_dir = ScrollOrderDir::for_local_label(label, &tether).await?;
        let order_field = ScrollOrderField::for_local_label(label, &tether).await?;

        if include.has_spam_and_trash()
            && let Some(alt_label) = alt_label
        {
            label = alt_label;
        }

        let source = DataScrollerSource::<MessageScrollData>::new(
            label,
            alt_label,
            unread,
            page_size,
            order_dir,
            order_field,
        );

        Self::new(ctx, source, page_size, alt_label.is_some()).await
    }

    pub async fn search(
        ctx: Weak<MailUserContext>,
        options: SearchOptions,
        include: IncludeSwitch,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<Message>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection().await?;
        let settings = MailSettings::get_or_default(&tether).await;

        let label = if include.has_spam_and_trash() {
            LabelId::all_mail()
        } else {
            settings.all_mail()
        };

        let alt_label = match settings.almost_all_mail {
            AlmostAllMail::AllMail => None,
            AlmostAllMail::AlmostAllMail => Some(LabelId::all_mail()),
        };

        let source = SearchScrollerSource::new(label, options, page_size);

        Self::new(ctx, source, page_size, alt_label.is_some()).await
    }

    /// If `id` points at the `All Mail` label, this function returns id of the
    /// `Almost All Mail` label; otherwise it returns `None`.
    async fn alternative_label(
        id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Option<LocalLabelId>, MailContextError> {
        let Some(id) = Label::local_id_counterpart(id, tether).await? else {
            return Ok(None);
        };

        if id == LabelId::almost_all_mail() {
            let id = Label::find_by_remote_id(LabelId::all_mail(), tether)
                .await?
                .map(|label| label.id());

            Ok(id)
        } else {
            Ok(None)
        }
    }

    async fn new<T>(
        ctx: Arc<MailUserContext>,
        source: T,
        page_size: usize,
        supports_include_filter: bool,
    ) -> Result<(Self, MailScrollerHandle<T::Item>), MailContextError>
    where
        T: MailScrollerSource,
    {
        let id = Uuid::new_v4();
        let ctx_weak = Arc::downgrade(&ctx);

        tracing::debug!(?id, "Creating MailScroller");

        let ScrollerWorkerHandle {
            command,
            ordered_command,
            updates,
            handle,
            aborts,
        } = ScrollerWorker::run(ctx_weak, source, page_size).await?;

        let event_service = ctx.core_context().event_service();
        let ordered_command_cloned = ordered_command.clone();

        if let Some(mut event_subscriber) = event_service.subscribe::<OnEnterForegroundEvent>() {
            ctx.spawn(async move {
                loop {
                    if event_subscriber.next().await.is_err() {
                        return;
                    }

                    tracing::debug!("Scroller {id} fetch new after enter foreground");

                    if Self::do_fetch_new(&ordered_command_cloned).is_err() {
                        return;
                    }
                }
            });
        }

        let ordered_command_cloned = ordered_command.clone();
        if let Some(mut event_subscriber) = event_service.subscribe::<OnForceEventPollEvent>() {
            ctx.spawn(async move {
                loop {
                    if event_subscriber.next().await.is_err() {
                        return;
                    }

                    tracing::debug!("Scroller {id} fetch new after force refresh event");

                    if Self::do_fetch_new(&ordered_command_cloned).is_err() {
                        return;
                    }
                }
            });
        }

        Ok((
            Self {
                id,
                command,
                ordered_command,
                supports_include_filter,
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

    pub fn fetch_new(&self) -> Result<(), MailContextError> {
        Self::do_fetch_new(&self.ordered_command)?;

        Ok(())
    }

    fn do_fetch_new(
        sender: &flume::Sender<ScrollerOrderedCommand>,
    ) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        tracing::trace!("Sending `FetchNew` command with uuid: {uuid}");

        sender
            .send(ScrollerOrderedCommand::FetchNew(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send fetch new command")))
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

    pub fn change_filter(&self, unread: ReadFilter) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        tracing::trace!("Sending `ChangeFilter` command with uuid: {uuid}");

        self.ordered_command
            .send(ScrollerOrderedCommand::ChangeFilter {
                src: ScrollerSource::ScrollEvent(uuid),
                unread,
            })
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send change filter command"))
            })?;

        Ok(())
    }

    pub fn change_include(&self, include: IncludeSwitch) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        tracing::trace!("Sending `ChangeInclude` command with uuid: {uuid}");

        self.ordered_command
            .send(ScrollerOrderedCommand::ChangeInclude {
                src: ScrollerSource::ScrollEvent(uuid),
                include,
            })
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send change include command"))
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

    pub fn supports_include_filter(&self) -> bool {
        self.supports_include_filter
    }

    pub fn terminate(&self) {
        for abort in &self.aborts {
            abort.abort();
        }
    }
}

pub struct ScrollerWorker<T: MailScrollerSource> {
    ctx: Weak<MailUserContext>,
    source: Arc<RwLock<T>>,
    task: MailPaginatorJoinHandle,
    execute_on_online: Option<AbortHandle>,
    items: Vec<T::Item>,
    page_size: usize,
    update: flume::Sender<ScrollerUpdate<T::Item>>,
    ordered_command_recv: flume::Receiver<ScrollerOrderedCommand>,
    ordered_command_send: flume::Sender<ScrollerOrderedCommand>,
}

impl<T: MailScrollerSource> Drop for ScrollerWorker<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.execute_on_online.take() {
            handle.abort();
        }
    }
}

impl<T: MailScrollerSource> ScrollerWorker<T> {
    async fn run(
        ctx: Weak<MailUserContext>,
        mut source: T,
        page_size: usize,
    ) -> Result<ScrollerWorkerHandle<T>, MailContextError> {
        let (update_sender, update_receiver) = flume::unbounded();
        let (command_sender, command_receiver) = flume::unbounded();
        let (ordered_command_sender, ordered_command_receiver) = flume::unbounded();
        let (invalidation_sender, invalidation_receiver) = flume::unbounded();
        let arc_ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let task = source.initialize(&arc_ctx, invalidation_sender).await?;
        let tables = source.watched_tables();

        let WatcherHandle {
            receiver: db_receiver,
            handle,
            ..
        } = arc_ctx
            .user_stash()
            .subscribe_to(move |sender| Box::new(MailScrollerWatcher { sender, tables }))
            .await?;

        let source = Arc::new(RwLock::new(source));

        let this = Self {
            ctx,
            source,
            page_size,
            task,
            execute_on_online: None,
            items: vec![],
            update: update_sender,
            ordered_command_recv: ordered_command_receiver,
            ordered_command_send: ordered_command_sender.clone(),
        };

        let aborts = this.spawn(
            command_receiver,
            ordered_command_sender.clone(),
            db_receiver,
            invalidation_receiver,
        )?;

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
        db_receiver: flume::Receiver<()>,
        invalidation_receiver: flume::Receiver<()>,
    ) -> Result<Vec<AbortHandle>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let mut aborts = vec![];
        let source_clone = self.source.clone();
        let weak_ctx = self.ctx.clone();

        // Ordered operations, these needs to be streamlined and not blocking other operations
        // thats why we are going to dedicate a separate task for them.
        let handle = ctx.spawn(async move {
            while let Ok(command) = self.ordered_command_recv.recv_async().await {
                // This prevents abusing the scroller by sending multiple commands
                // in a row. We do not want and need to handle all of them one by one.
                let commands = self.ordered_command_recv.drain().collect_vec();
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
                    r = db_receiver.recv_async() => {
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
                let result =
                    self.fetch_more(source)
                        .await
                        .unwrap_or_else(|e| ScrollerUpdate::Error {
                            src: source,
                            error: e,
                        });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send(result)
                        .map_err(|e| anyhow!("Failed to send fetch more update: {e:?}"))?;
                }
            }

            ScrollerOrderedCommand::Refresh(source) => {
                let result =
                    self.refresh(false, source)
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
                let result =
                    self.refresh(true, source)
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
                let items_update = self.get_items(src);

                self.update
                    .send_async(items_update)
                    .await
                    .map_err(|e| anyhow!("Failed to send get items update: {e:?}"))?;
            }

            ScrollerOrderedCommand::FetchNew(src) => {
                let result = self
                    .fetch_new(src)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                self.update
                    .send_async(result)
                    .await
                    .map_err(|e| anyhow!("Failed to send fetch new update: {e:?}"))?;
            }

            ScrollerOrderedCommand::ChangeFilter { src, unread } => {
                let result = self
                    .change_filter(src, unread)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send_async(result)
                        .await
                        .map_err(|e| anyhow!("Failed to send change filter update: {e:?}"))?;
                }
            }

            ScrollerOrderedCommand::ChangeInclude { src, include } => {
                self.change_include(src, include).await;

                let result = self
                    .clear_cursor(src)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                self.update
                    .send_async(result)
                    .await
                    .map_err(|e| anyhow!("Failed to send clear cursor update: {e:?}"))?;
            }

            ScrollerOrderedCommand::ClearCursor(src) => {
                let result = self
                    .clear_cursor(src)
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
        let items = self.sync_next().await?;
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let (seen, synced, total, has_more_in_source) = {
            let source = self.source.read().await;
            let seen = source.seen_total(&ctx).await?;
            let synced = source.synced_total(&ctx).await?;
            let total = source.all_total(&ctx).await?;
            let has_more = source.has_more(&ctx).await?;
            (seen, synced, total, has_more)
        };
        let has_more_in_label = seen < total;

        tracing::info!(
            "Fetch stats - seen/synced/total: {seen}/{synced}/{total}. Has more - source/label: {has_more_in_source}/{has_more_in_label}"
        );

        if items.is_empty() && has_more_in_label {
            if self.execute_on_online.is_none() {
                tracing::debug!("No items to return, requesting additional fetch more");
                let ctx_clone = ctx.clone();
                let channel = self.ordered_command_send.clone();
                let handle = ctx.spawn(async move {
                    ctx_clone
                        .network_monitor_service()
                        .network_status_observer()
                        .wait_until_online()
                        .await;
                    Self::schedule_fetch_more(&channel, call_src).await;
                });
                self.execute_on_online = Some(handle.abort_handle());
            }

            let is_offline = ctx.network_monitor_service().is_os_offline();

            if self.task.is_none() {
                if is_offline {
                    tracing::warn!("Scroller is offline, will not progress any further");
                    // We will not progress any further without task,
                    // and task will be spawned only when we are online,
                    // lets wait for another call.
                    return Err(MailContextError::no_connection());
                } else {
                    tracing::warn!("We couldn't sync new items");
                }
            }
        }

        if items.is_empty() {
            tracing::debug!("No new items fetched");
            Ok(ScrollerUpdate::None(call_src))
        } else {
            if let Some(handle) = self.execute_on_online.take() {
                handle.abort();
            }

            tracing::debug!("Append: items number: {}", items.len());
            self.items.extend(items.clone());
            Ok(ScrollerUpdate::Append {
                src: call_src,
                items,
            })
        }
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn fetch_new(
        &mut self,
        src: ScrollerSource,
    ) -> Result<ScrollerUpdate<T::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let mut task = self.source.write().await.sync_new(&ctx).await?;
        Self::await_task(&mut task).await?;

        self.refresh(false, src).await
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn refresh(
        &mut self,
        force: bool,
        src: ScrollerSource,
    ) -> Result<ScrollerUpdate<T::Item>, MailContextError> {
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
        } else {
            tracing::debug!("Calculating diff...");
            let update = calculate_scroller_update(&self.items, &visible_items, src);
            self.items = visible_items;

            update
        };

        // Make sure we can see the first page.
        self.try_fetch_first_page(src).await?;

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
        unread: ReadFilter,
    ) -> Result<ScrollerUpdate<T::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        tracing::debug!("Changing filter to {unread:?}");
        // We drop the previous task, we should not wait for it.
        let _ = self.task.take();
        self.task = self
            .source
            .write()
            .await
            .change_filter(&ctx, unread)
            .await?;
        self.items.clear();
        self.fetch_more(src).await?;
        self.refresh(true, src).await
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn change_include(&mut self, src: ScrollerSource, include: IncludeSwitch) {
        tracing::debug!("Changing include to {include:?}");

        self.source.write().await.change_include(include);
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
        self.task = self.source.write().await.clear_cursor(&ctx).await?;
        self.items.clear();
        self.fetch_more(src).await?;
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

    async fn try_fetch_first_page(&mut self, src: ScrollerSource) -> Result<(), MailContextError> {
        let total = Self::total(&self.source, &self.ctx).await?;
        let seen = Self::seen(&self.source, &self.ctx).await?;
        let page_size = self.page_size as u64;
        let cant_see_first_page = total > 0 && seen < page_size && seen < total;

        if cant_see_first_page {
            tracing::info!("We do not see the first page, requesting fetch more");
            Self::schedule_fetch_more(&self.ordered_command_send, src).await;
        }

        Ok(())
    }

    async fn sync_next(&mut self) -> Result<Vec<T::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let result = self.wait_for_request().await;

        if let Err(e) = &result {
            tracing::error!("Error occurred while waiting for previous request: {e}");
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

        if items.is_empty() && self.task.is_none() {
            let status = ctx.network_monitor_service().combined_status();
            tracing::warn!(
                "No items and no task to return - status: {status:?}, previous fetch result: {result:?}"
            );
            result?;
        }

        Ok(items)
    }

    async fn wait_for_request(&mut self) -> Result<(), MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let is_online = ctx.network_monitor_service().is_os_online();

        if self.task.is_some() && is_online {
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

    async fn schedule_fetch_more(
        channel: &flume::Sender<ScrollerOrderedCommand>,
        src: ScrollerSource,
    ) {
        let _ = channel
            .send_async(ScrollerOrderedCommand::FetchMore(src))
            .await
            .inspect_err(|e| tracing::error!("Failed to schedule fetch more command: {e:?}"));
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
    FetchNew(ScrollerSource),
    Refresh(ScrollerSource),
    ForceRefresh(ScrollerSource),
    GetItems(ScrollerSource),
    ChangeFilter {
        src: ScrollerSource,
        unread: ReadFilter,
    },
    ChangeInclude {
        src: ScrollerSource,
        include: IncludeSwitch,
    },
    ClearCursor(ScrollerSource),
}

fn calculate_scroller_update<T>(old: &[T], new: &[T], src: ScrollerSource) -> ScrollerUpdate<T>
where
    T: ScrollerEq + Clone,
{
    let prefix_count = old
        .iter()
        .zip(new.iter())
        .take_while(|(a, b)| a.scroller_eq(b))
        .count();

    tracing::debug!("Prefix count: {prefix_count}");

    if old.len() == new.len() && prefix_count == old.len() {
        tracing::debug!("No update required");
        return ScrollerUpdate::None(src);
    } else if prefix_count == old.len() {
        let items = new[prefix_count..].to_vec();
        tracing::debug!("Append: items number: {}", items.len());
        return ScrollerUpdate::Append { src, items };
    }

    let suffix_count = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a.scroller_eq(b))
        .count();

    tracing::debug!("Suffix count: {suffix_count}");

    match (prefix_count, suffix_count) {
        (prefix_count, 0) => {
            let idx = prefix_count;
            let items = new[prefix_count..].to_vec();
            tracing::debug!("Replace from: {idx}, items number: {}", items.len());
            ScrollerUpdate::ReplaceFrom { src, idx, items }
        }
        (0, suffix_count) => {
            let idx = old.len().saturating_sub(suffix_count);
            let items = {
                let idx = new.len().saturating_sub(suffix_count);
                new[..idx].to_vec()
            };
            tracing::debug!("Replace before: {idx}, items number: {}", items.len());
            ScrollerUpdate::ReplaceBefore { src, idx, items }
        }
        (prefix_count, suffix_count) => {
            let from = prefix_count;
            let to = old.len().saturating_sub(suffix_count);
            let items = {
                let to = new.len().saturating_sub(suffix_count);
                new[from..to].to_vec()
            };
            tracing::debug!("Replace range: {from}..{to}, items number: {}", items.len());
            ScrollerUpdate::ReplaceRange {
                src,
                from,
                to,
                items,
            }
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl<T: Clone> Clone for ScrollerUpdate<T> {
    fn clone(&self) -> Self {
        match self {
            ScrollerUpdate::None(src) => ScrollerUpdate::None(*src),
            ScrollerUpdate::Append { src, items } => ScrollerUpdate::Append {
                src: *src,
                items: items.clone(),
            },
            ScrollerUpdate::ReplaceFrom { src, idx, items } => ScrollerUpdate::ReplaceFrom {
                src: *src,
                idx: *idx,
                items: items.clone(),
            },
            ScrollerUpdate::ReplaceBefore { src, idx, items } => ScrollerUpdate::ReplaceBefore {
                src: *src,
                idx: *idx,
                items: items.clone(),
            },
            ScrollerUpdate::ReplaceRange {
                src,
                from,
                to,
                items,
            } => ScrollerUpdate::ReplaceRange {
                src: *src,
                from: *from,
                to: *to,
                items: items.clone(),
            },
            ScrollerUpdate::Error { .. } => panic!("Cannot clone error update"),
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

    impl ScrollerEq for i32 {
        fn scroller_eq(&self, other: &Self) -> bool {
            *self == *other
        }
    }

    // Test cases for calculate_scroller_update function
    #[test_case(vec![], vec![] => matches ScrollerUpdate::None(_); "Test 1: empty to empty")]
    #[test_case(vec![], vec![1] => matches ScrollerUpdate::Append { items, .. } if items == vec![1]; "Test 2: empty to single item")]
    #[test_case(vec![], vec![1, 2, 3] => matches ScrollerUpdate::Append { items, .. } if items == vec![1, 2, 3]; "Test 3: empty to multiple items")]
    #[test_case(vec![1], vec![] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items.is_empty(); "Test 4: single item to empty")]
    #[test_case(vec![1, 2, 3], vec![] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items.is_empty(); "Test 5: multiple items to empty")]
    #[test_case(vec![1], vec![1] => matches ScrollerUpdate::None(_); "Test 6: same single item")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 3] => matches ScrollerUpdate::None(_); "Test 7: same multiple items")]
    // Items added at the beginning
    #[test_case(vec![1, 2, 3], vec![0, 1, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![0]; "Test 8: add one item at beginning")]
    #[test_case(vec![1, 2, 3], vec![0, -1, 1, 2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![0, -1]; "Test 9: add two items at beginning")]
    #[test_case(vec![3, 4, 5], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![1, 2]; "Test 10: add items at beginning with all suffix common")]
    // Items added at the end
    #[test_case(vec![1, 2, 3], vec![1, 2, 3, 4] => matches ScrollerUpdate::Append { items, .. } if items == vec![4]; "Test 11: add one item at end")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::Append { items, .. } if items == vec![4, 5]; "Test 12: add two items at end")]
    // Items added in the middle
    #[test_case(vec![1, 3], vec![1, 2, 3] => matches ScrollerUpdate::ReplaceRange { from: 1, to: 1, items, .. } if items == vec![2]; "Test 13: add item in middle")]
    #[test_case(vec![1, 4], vec![1, 2, 3, 4] => matches ScrollerUpdate::ReplaceRange { from: 1, to: 1, items, .. } if items == vec![2, 3]; "Test 14: add two items in middle")]
    #[test_case(vec![1, 4, 5], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::ReplaceRange { from: 1, to: 1, items, .. } if items == vec![2, 3]; "Test 14a: add two items in middle")]
    // Items removed from beginning
    #[test_case(vec![1, 2, 3], vec![2, 3] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items.is_empty(); "Test 15: remove one item from beginning")]
    #[test_case(vec![1, 2, 3, 4], vec![3, 4] => matches ScrollerUpdate::ReplaceBefore { idx: 2, items, .. } if items.is_empty(); "Test 16: remove two items from beginning")]
    // Items removed from end
    #[test_case(vec![1, 2, 3], vec![1, 2] => matches ScrollerUpdate::ReplaceFrom { idx: 2, items, .. } if items.is_empty(); "Test 17: remove one item from end")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 2] => matches ScrollerUpdate::ReplaceFrom { idx: 2, items, .. } if items.is_empty(); "Test 18: remove two items from end")]
    // Items removed from middle
    #[test_case(vec![1, 2, 3], vec![1, 3] => matches ScrollerUpdate::ReplaceRange { from: 1, to: 2, items, .. } if items.is_empty(); "Test 19: remove item from middle")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 4] => matches ScrollerUpdate::ReplaceRange { from: 1, to: 3, items, .. } if items.is_empty(); "Test 20: remove two items from middle")]
    #[test_case(vec![1, 2, 3, 4, 5], vec![1, 4, 5] => matches ScrollerUpdate::ReplaceRange { from: 1, to: 3, items, .. } if items.is_empty(); "Test 20a: remove two items from middle")]
    // Items replaced
    #[test_case(vec![1, 2, 3], vec![1, 4, 3] => matches ScrollerUpdate::ReplaceRange { from: 1, to: 2, items, .. } if items == vec![4]; "Test 21: replace item in middle")]
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
    #[test_case(vec![1], vec![1, 2] => matches ScrollerUpdate::Append { items, .. } if items == vec![2]; "Test 30: single to two elements")]
    #[test_case(vec![1, 2], vec![1] => matches ScrollerUpdate::ReplaceFrom { idx: 1, items, .. } if items.is_empty(); "Test 31: two to single element")]
    #[test_case(vec![1], vec![2] => matches ScrollerUpdate::ReplaceFrom { idx: 0, items, .. } if items == vec![2]; "Test 32: single element replacement")]
    // Cases that test the 50% threshold logic
    #[test_case(vec![1, 2, 3, 4], vec![0, 2, 3, 4] => matches ScrollerUpdate::ReplaceBefore { idx: 1, items, .. } if items == vec![0]; "Test 33: suffix common >= 50% triggers ReplaceBefore")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 0, 0, 0] => matches ScrollerUpdate::ReplaceFrom { idx: 1, items, .. } if items == vec![0, 0, 0]; "Test 34: prefix common > suffix common")]
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![0, 0, 0, 4, 5, 6] => matches ScrollerUpdate::ReplaceBefore { idx: 3, items, .. } if items == vec![0, 0, 0]; "Test 35: suffix wins over prefix")]
    // Large vectors to test performance characteristics
    #[test_case((1..=100).collect::<Vec<_>>(), (0..=100).collect::<Vec<_>>() => matches ScrollerUpdate::ReplaceBefore { idx: 0, items, .. } if items == vec![0]; "Test 36: large vector add at beginning")]
    #[test_case((1..=100).collect::<Vec<_>>(), (1..=101).collect::<Vec<_>>() => matches ScrollerUpdate::Append { items, .. } if items == vec![101]; "Test 37: large vector add at end")]

    // Test the actual function
    fn test_calculate_scroller_update(old: Vec<i32>, new: Vec<i32>) -> ScrollerUpdate<i32> {
        let result = calculate_scroller_update(&old, &new, test_source());
        let actual = apply_scroller_update(old, &result);
        assert_eq!(actual, new);
        result
    }

    fn apply_scroller_update(mut current: Vec<i32>, update: &ScrollerUpdate<i32>) -> Vec<i32> {
        match update {
            ScrollerUpdate::None(_) => current,
            ScrollerUpdate::Append { items, .. } => {
                current.extend(items.clone());
                current
            }
            ScrollerUpdate::ReplaceFrom { idx, items, .. } => {
                current.splice(idx.., items.clone());
                current
            }
            ScrollerUpdate::ReplaceBefore { idx, items, .. } => {
                current.splice(..idx, items.clone());
                current
            }
            ScrollerUpdate::ReplaceRange {
                from, to, items, ..
            } => {
                current.splice(from..to, items.clone());
                current
            }
            ScrollerUpdate::Error { .. } => current,
        }
    }

    #[test]
    fn test_scroller_source_is_preserved() {
        let src = ScrollerSource::Database;
        let result = calculate_scroller_update(&[1, 2], &[1, 2, 3], src);

        match result {
            ScrollerUpdate::Append {
                src: result_src, ..
            } => {
                assert_eq!(result_src, src);
            }
            _ => panic!("Expected Append variant"),
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
