mod alternative_labels;
mod mail_scroller_source;
mod mail_scroller_watcher;

pub use self::alternative_labels::AlternativeLabels;
pub use self::mail_scroller_source::*;
pub use self::mail_scroller_watcher::*;
use crate::Mailbox;
use crate::datatypes::labels::{ScrollOrderDir, ScrollOrderField};
use crate::datatypes::{ContextualConversation, IncludeSwitch, ReadFilter, SearchOptions};
use crate::mail_cursor::MailCursor;
use crate::models::MailSettings;
use crate::models::{ConversationScrollData, Message, MessageScrollData};
use crate::traits::ScrollerEq;
use crate::{MailContextError, MailUserContext};
use anyhow::anyhow;
use derivative::Derivative;
use derive_more::Display;
use futures::select;
use itertools::Itertools;
use parking_lot::RwLock as SyncRwLock;
use proton_core_common::app_events::{OnEnterForegroundEvent, OnForceEventPollEvent};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use proton_core_common::models::ModelIdExtension;
use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use stash::stash::WatcherHandle;
use std::sync::{Arc, Weak};
use tokio::sync::{RwLock, oneshot};
use tokio::task::AbortHandle;
use tracing::debug;
use tracing::info;
use tracing::instrument;
use tracing::trace;
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
pub enum ScrollerStatusUpdate {
    FetchNewStart(ScrollerSource),
    FetchNewEnd(ScrollerSource),
}

impl<T> From<ScrollerStatusUpdate> for ScrollerUpdate<T> {
    fn from(value: ScrollerStatusUpdate) -> Self {
        Self::Status(value)
    }
}

#[derive(Debug)]
pub enum ScrollerListUpdate<T> {
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
}

impl<T> From<ScrollerListUpdate<T>> for ScrollerUpdate<T> {
    fn from(value: ScrollerListUpdate<T>) -> Self {
        Self::List(value)
    }
}

#[derive(Debug)]
pub enum ScrollerUpdate<T> {
    Status(ScrollerStatusUpdate),
    List(ScrollerListUpdate<T>),
    Error {
        src: ScrollerSource,
        error: MailContextError,
    },
}

impl<T> ScrollerUpdate<T> {
    pub fn is_none(&self) -> bool {
        matches!(self, ScrollerUpdate::List(ScrollerListUpdate::None(_)))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, ScrollerUpdate::Error { .. })
    }

    pub fn is_status_update(&self) -> bool {
        matches!(self, ScrollerUpdate::Status(_))
    }

    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn src(&self) -> &ScrollerSource {
        match self {
            ScrollerUpdate::List(update) => match update {
                ScrollerListUpdate::None(src) => src,
                ScrollerListUpdate::Append { src, .. } => src,
                ScrollerListUpdate::ReplaceFrom { src, .. } => src,
                ScrollerListUpdate::ReplaceBefore { src, .. } => src,
                ScrollerListUpdate::ReplaceRange { src, .. } => src,
            },
            ScrollerUpdate::Status(update) => match update {
                ScrollerStatusUpdate::FetchNewStart(src) => src,
                ScrollerStatusUpdate::FetchNewEnd(src) => src,
            },
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
pub struct MailScroller<T>
where
    T: MailScrollerItem,
{
    id: Uuid,
    ctx: Weak<MailUserContext>,
    command: flume::Sender<ScrollerCommand<T>>,
    ordered_command: flume::Sender<ScrollerOrderedCommand>,
    aborts: Vec<AbortHandle>,
}

impl MailScroller<ContextualConversation> {
    pub async fn conversations(
        ctx: Weak<MailUserContext>,
        label: LocalLabelId,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<ContextualConversation>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection().await?;
        let order_dir = ScrollOrderDir::for_local_label(label, &tether).await?;
        let order_field = ScrollOrderField::for_local_label(label, &tether).await?;
        let unread = ReadFilter::All;
        let source = DataScrollerSource::<ConversationScrollData>::new(
            label,
            unread,
            page_size,
            order_dir,
            order_field,
        );

        Self::new(ctx, source, page_size, label).await
    }
}

impl MailScroller<Message> {
    pub async fn messages(
        ctx: Weak<MailUserContext>,
        label: LocalLabelId,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<Message>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection().await?;
        let order_dir = ScrollOrderDir::for_local_label(label, &tether).await?;
        let order_field = ScrollOrderField::for_local_label(label, &tether).await?;
        let unread = ReadFilter::All;
        let source = DataScrollerSource::<MessageScrollData>::new(
            label,
            unread,
            page_size,
            order_dir,
            order_field,
        );

        Self::new(ctx, source, page_size, label).await
    }

    pub async fn search(
        ctx: Weak<MailUserContext>,
        options: SearchOptions,
        page_size: usize,
    ) -> Result<(Self, MailScrollerHandle<Message>), MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection().await?;
        let label = MailSettings::get_or_default(&tether).await.all_mail();
        let label = Label::remote_id_counterpart(label, &tether)
            .await?
            .expect("System labels should always have a local counterpart");
        let source = SearchScrollerSource::new(label, options, page_size);

        Self::new(ctx, source, page_size, label).await
    }
}

impl<T> MailScroller<T>
where
    T: MailScrollerItem,
{
    pub(crate) async fn new<S>(
        ctx: Arc<MailUserContext>,
        source: S,
        page_size: usize,
        label: LocalLabelId,
    ) -> Result<(Self, MailScrollerHandle<T>), MailContextError>
    where
        S: MailScrollerSource<Item = T>,
    {
        let id = Uuid::new_v4();
        let ctx_weak = Arc::downgrade(&ctx);

        debug!(?id, "Creating MailScroller");

        let ScrollerWorkerHandle {
            command,
            ordered_command,
            updates,
            handle,
            aborts,
        } = ScrollerWorker::run(ctx_weak.clone(), source, page_size, label).await?;

        let event_service = ctx.core_context().event_service();
        let ordered_command_cloned = ordered_command.clone();

        if let Some(mut event_subscriber) = event_service.subscribe::<OnEnterForegroundEvent>() {
            ctx.spawn(async move {
                loop {
                    if event_subscriber.next().await.is_err() {
                        return;
                    }

                    debug!("Scroller {id} fetch new after enter foreground");

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

                    debug!("Scroller {id} fetch new after force refresh event");

                    if Self::do_fetch_new(&ordered_command_cloned).is_err() {
                        return;
                    }
                }
            });
        }

        Ok((
            Self {
                id,
                ctx: ctx_weak,
                command,
                ordered_command,
                aborts,
            },
            MailScrollerHandle { updates, handle },
        ))
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub async fn has_more(&self) -> Result<bool, MailContextError> {
        let (sender, receiver) = oneshot::channel();

        debug!("Sending `HasMore` command");

        self.command
            .send(ScrollerCommand::HasMore(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send has more command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive has more response")))?
    }

    pub async fn cursor(
        self: &Arc<Self>,
        looking_at: T::Id,
    ) -> Result<MailCursor<T>, MailContextError> {
        let (sender, receiver) = oneshot::channel();

        debug!("Sending `Cursor` command");

        self.command
            .send(ScrollerCommand::Cursor(self.clone(), looking_at, sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send `cursor` command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive `cursor` response")))
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn fetch_more(&self, tx: Option<oneshot::Sender<()>>) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `FetchMore` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::FetchMore {
                src: ScrollerSource::ScrollEvent(uuid),
                tx,
            })
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send fetch more command")))?;

        Ok(())
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn fetch_new(&self) -> Result<(), MailContextError> {
        Self::do_fetch_new(&self.ordered_command)?;

        Ok(())
    }

    fn do_fetch_new(
        sender: &flume::Sender<ScrollerOrderedCommand>,
    ) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `FetchNew` command");

        sender
            .send(ScrollerOrderedCommand::FetchNew(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send fetch new command")))
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn refresh(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `Refresh` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::Refresh(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send refresh command")))?;

        Ok(())
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn force_refresh(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `ForceRefresh` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::ForceRefresh(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send force refresh command"))
            })?;

        Ok(())
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn get_items(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `GetItems` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::GetItems(
                ScrollerSource::ScrollEvent(uuid),
            ))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get items command")))?;

        Ok(())
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn change_filter(&self, unread: ReadFilter) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `ChangeFilter` command");

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

    pub fn change_label(
        &self,
        mailbox: &Mailbox,
        label: LocalLabelId,
    ) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `ChangeLabel` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::ChangeLabel {
                src: ScrollerSource::ScrollEvent(uuid),
                label,
            })
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send change label command")))?;

        if let Some(ctx) = self.ctx.upgrade() {
            let mailbox = mailbox.clone();
            let ctx2 = ctx.clone();

            ctx2.spawn(async move {
                let Ok(tether) = ctx.user_stash().connection().await else {
                    tracing::error!("Failed to get connection");
                    return;
                };

                if let Err(err) = mailbox.change_label(&tether, label).await {
                    tracing::error!("Failed to change label: {}", err);
                }
            });
        }

        Ok(())
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub fn change_include(
        &self,
        mailbox: &Mailbox,
        include: IncludeSwitch,
    ) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();

        debug!(?uuid, "Sending `ChangeInclude` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::ChangeInclude {
                tx,
                src: ScrollerSource::ScrollEvent(uuid),
                include,
            })
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send change include command"))
            })?;

        if let Some(ctx) = self.ctx.upgrade() {
            let mailbox = mailbox.clone();
            let ctx2 = ctx.clone();

            ctx2.spawn(async move {
                let Ok(label_id) = rx.await else {
                    tracing::error!("Failed to receive label ID");
                    return;
                };

                let Ok(tether) = ctx.user_stash().connection().await else {
                    tracing::error!("Failed to get connection");
                    return;
                };

                if let Err(err) = mailbox.change_label(&tether, label_id).await {
                    tracing::error!("Failed to change label: {}", err);
                }
            });
        }

        Ok(())
    }

    pub fn change_keywords(&self, keywords: SearchOptions) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `ChangeKeywords` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::ChangeKeywords {
                src: ScrollerSource::ScrollEvent(uuid),
                keywords,
            })
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send change keywords command"))
            })?;

        Ok(())
    }

    pub fn clear(&self) -> Result<(), MailContextError> {
        let uuid = Uuid::new_v4();

        debug!(?uuid, "Sending `Clear` command");

        self.ordered_command
            .send(ScrollerOrderedCommand::Clear(ScrollerSource::ScrollEvent(
                uuid,
            )))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send clear command")))?;

        Ok(())
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub async fn total(&self) -> Result<u64, MailContextError> {
        let (sender, receiver) = oneshot::channel();

        debug!("Sending `GetTotal` query");

        self.command
            .send(ScrollerCommand::GetTotal(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get total command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive total response")))?
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub async fn seen(&self) -> Result<u64, MailContextError> {
        let (sender, receiver) = oneshot::channel();

        debug!("Sending `GetSeen` query");

        self.command
            .send(ScrollerCommand::GetSeen(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get seen command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive seen response")))?
    }

    #[instrument(skip_all, fields(id = ?self.id))]
    pub async fn synced(&self) -> Result<u64, MailContextError> {
        let (sender, receiver) = oneshot::channel();

        debug!("Sending `GetSynced` query");

        self.command
            .send(ScrollerCommand::GetSynced(sender))
            .map_err(|_| MailContextError::Other(anyhow!("Failed to send get synced command")))?;

        receiver
            .await
            .map_err(|_| MailContextError::Other(anyhow!("Failed to receive synced response")))?
    }

    pub async fn supports_include_filter(&self) -> Result<bool, MailContextError> {
        let (tx, rx) = oneshot::channel();

        self.ordered_command
            .send(ScrollerOrderedCommand::AlternativeLabels(tx))
            .map_err(|_| {
                MailContextError::Other(anyhow!("Failed to send supports include filter command"))
            })?;

        let alternative_labels = rx.await.map_err(|_| {
            MailContextError::Other(anyhow!(
                "Failed to receive supports include filter response"
            ))
        })?;

        Ok(alternative_labels.supports_include_filter())
    }

    pub fn terminate(&self) {
        for abort in &self.aborts {
            abort.abort();
        }
    }
}

impl<T> Drop for MailScroller<T>
where
    T: MailScrollerItem,
{
    #[instrument(skip_all, fields(id = ?self.id))]
    fn drop(&mut self) {
        debug!(
            "Dropping MailScroller, got {} task(s) to abort",
            self.aborts.len()
        );

        self.terminate()
    }
}

pub struct MailScrollerHandle<T> {
    pub updates: flume::Receiver<ScrollerUpdate<T>>,
    pub handle: DropRemoveTableObserverHandle,
}

struct ScrollerWorker<S>
where
    S: MailScrollerSource,
{
    ctx: Weak<MailUserContext>,
    source: Arc<RwLock<S>>,
    task: MailPaginatorJoinHandle,
    execute_on_online: Option<AbortHandle>,
    items: Arc<SyncRwLock<Vec<S::Item>>>,
    page_size: usize,
    alternative_labels: AlternativeLabels,
    update: flume::Sender<ScrollerUpdate<S::Item>>,
    ordered_command_recv: flume::Receiver<ScrollerOrderedCommand>,
    ordered_command_send: flume::Sender<ScrollerOrderedCommand>,
}

impl<S> Drop for ScrollerWorker<S>
where
    S: MailScrollerSource,
{
    fn drop(&mut self) {
        if let Some(handle) = self.execute_on_online.take() {
            handle.abort();
        }
    }
}

impl<S> ScrollerWorker<S>
where
    S: MailScrollerSource,
{
    async fn run(
        ctx: Weak<MailUserContext>,
        mut source: S,
        page_size: usize,
        label: LocalLabelId,
    ) -> Result<ScrollerWorkerHandle<S>, MailContextError> {
        let (update_sender, update_receiver) = flume::unbounded();
        let (command_sender, command_receiver) = flume::unbounded();
        let (ordered_command_sender, ordered_command_receiver) = flume::unbounded();
        let (invalidation_sender, invalidation_receiver) = flume::unbounded();
        let arc_ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let task = source.initialize(&arc_ctx, invalidation_sender).await?;
        let tables = source.watched_tables();
        let tether = arc_ctx.user_stash().connection().await?;
        let alternative_labels = AlternativeLabels::new(label, &tether).await?;

        let WatcherHandle {
            receiver, handle, ..
        } = arc_ctx
            .user_stash()
            .subscribe_to(move |sender| Box::new(MailScrollerWatcher { sender, tables }))
            .await?;

        let source = Arc::new(RwLock::new(source));
        let items = Arc::new(SyncRwLock::new(vec![]));

        let this = Self {
            ctx,
            source,
            page_size,
            task,
            execute_on_online: None,
            items,
            alternative_labels,
            update: update_sender,
            ordered_command_recv: ordered_command_receiver,
            ordered_command_send: ordered_command_sender.clone(),
        };

        let aborts = this.spawn(
            command_receiver,
            ordered_command_sender.clone(),
            receiver,
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
        command_receiver: flume::Receiver<ScrollerCommand<S::Item>>,
        ordered_command_sender: flume::Sender<ScrollerOrderedCommand>,
        db_receiver: flume::Receiver<()>,
        invalidation_receiver: flume::Receiver<()>,
    ) -> Result<Vec<AbortHandle>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let mut aborts = vec![];
        let source_clone = self.source.clone();
        let items_clone = Arc::clone(&self.items);
        let weak_ctx = self.ctx.clone();

        // Ordered operations, these needs to be streamlined and not blocking other operations
        // thats why we are going to dedicate a separate task for them.
        let handle = ctx.spawn(async move {
            while let Ok(command) = self.ordered_command_recv.recv_async().await {
                // This prevents abusing the scroller by sending multiple commands
                // in a row. We do not want and need to handle all of them one by one.
                let commands = self.ordered_command_recv.drain().collect_vec();
                trace!("Handling ordered commands: {:?}", commands);
                let mut processed = 0;
                for command in Some(command).into_iter().chain(commands).dedup() {
                    trace!("Processing ordered command: {:?}", command);
                    if let Err(e) = self.handle_ordered_command(command).await {
                        tracing::error!("Failed to handle ordered command: {e:?}");
                    }
                    processed += 1;
                }
                trace!("Processed {} ordered commands", processed);
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
                        if let Err(e) = Self::handle_command(r.unwrap(), &source_clone, items_clone.clone(), &weak_ctx).await {
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
            ScrollerOrderedCommand::FetchMore { src, tx } => {
                let result = self
                    .fetch_more(src)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send(result)
                        .map_err(|e| anyhow!("Failed to send fetch more update: {e:?}"))?;
                }

                if let Some(tx) = tx {
                    _ = tx.send(());
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
                self.update
                    .send_async(ScrollerStatusUpdate::FetchNewStart(src).into())
                    .await
                    .map_err(|e| anyhow!("Failed to send fetch new update: {e:?}"))?;

                let result = self
                    .fetch_new(src)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                self.update
                    .send_async(ScrollerStatusUpdate::FetchNewEnd(src).into())
                    .await
                    .map_err(|e| anyhow!("Failed to send fetch new update: {e:?}"))?;

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

            ScrollerOrderedCommand::ChangeLabel { src, label } => {
                let result = self
                    .change_label(src, label, Some(ReadFilter::All))
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                if !result.is_error() {
                    // Its not the end of the world if we fail to recalculate alternative labels.
                    // So lets just log the error and continue.
                    self.recalculate_alternative_labels(label)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::error!("Failed to recalculate alternative labels: {e:?}");
                        });
                }

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send_async(result)
                        .await
                        .map_err(|e| anyhow!("Failed to send change label update: {e:?}"))?;
                }
            }

            ScrollerOrderedCommand::ChangeInclude { tx, src, include } => {
                let result = self
                    .change_include(src, include)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                if !result.is_error() {
                    let label = self.include_to_label(include).await;
                    tx.send(label)
                        .map_err(|e| anyhow!("Failed to send change include update: {e:?}"))?;
                }

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send_async(result)
                        .await
                        .map_err(|e| anyhow!("Failed to send change label update: {e:?}"))?;
                }
            }

            ScrollerOrderedCommand::ChangeKeywords { src, keywords } => {
                let result = self
                    .change_keywords(src, keywords)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                if result.is_some() || result.is_scroll_event() {
                    self.update
                        .send_async(result)
                        .await
                        .map_err(|e| anyhow!("Failed to send change keywords update: {e:?}"))?;
                }
            }

            ScrollerOrderedCommand::Clear(src) => {
                let result = self
                    .clear(src)
                    .await
                    .unwrap_or_else(|e| ScrollerUpdate::Error { src, error: e });

                self.update
                    .send_async(result)
                    .await
                    .map_err(|e| anyhow!("Failed to send clear cursor update: {e:?}"))?;
            }

            ScrollerOrderedCommand::AlternativeLabels(tx) => {
                tx.send(self.alternative_labels)
                    .map_err(|e| anyhow!("Failed to send alternative label update: {e:?}"))?;
            }
        }

        Ok(())
    }

    async fn handle_command(
        command: ScrollerCommand<S::Item>,
        source: &RwLock<S>,
        items: Arc<SyncRwLock<Vec<S::Item>>>,
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
            ScrollerCommand::Cursor(scroller, looking_at, sender) => {
                let cursor = MailCursor::new(looking_at, items.clone(), scroller);
                sender
                    .send(cursor)
                    .map_err(|_| anyhow!("Fail to send `cursor`"))?;
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, fields(src=%call_src))]
    async fn fetch_more(
        &mut self,
        call_src: ScrollerSource,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
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

        info!(
            "Fetch stats - seen/synced/total: {seen}/{synced}/{total}. Has more - source/label: {has_more_in_source}/{has_more_in_label}"
        );

        if items.is_empty() && has_more_in_label {
            if self.execute_on_online.is_none() {
                debug!("No items to return, requesting additional fetch more");
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
            debug!("No new items fetched");

            Ok(ScrollerListUpdate::None(call_src).into())
        } else {
            if let Some(handle) = self.execute_on_online.take() {
                handle.abort();
            }

            debug!("Append: items number: {}", items.len());

            self.items.write().extend(items.clone());

            Ok(ScrollerListUpdate::Append {
                src: call_src,
                items,
            }
            .into())
        }
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn fetch_new(
        &mut self,
        src: ScrollerSource,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
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
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let visible_items = { self.source.read().await.visible_items(&ctx).await? };

        info!(
            "Refresh stats - new count: {}, current count: {}",
            visible_items.len(),
            self.items.read().len()
        );

        let update = if force {
            *self.items.write() = visible_items.clone();

            ScrollerListUpdate::ReplaceFrom {
                src,
                idx: 0,
                items: visible_items,
            }
            .into()
        } else {
            debug!("Calculating diff...");

            let update = calculate_scroller_update(&self.items.read(), &visible_items, src);

            *self.items.write() = visible_items;

            update
        };

        // Make sure we can see the first page.
        self.try_fetch_first_page(src).await?;

        Ok(update)
    }

    fn get_items(&self, src: ScrollerSource) -> ScrollerUpdate<S::Item> {
        let items = self.items.read().clone();

        ScrollerListUpdate::ReplaceFrom { src, idx: 0, items }.into()
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn change_filter(
        &mut self,
        src: ScrollerSource,
        unread: ReadFilter,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        debug!("Changing filter to {unread:?}");
        let _ = self.task.take();
        self.task = self
            .source
            .write()
            .await
            .change_state(&ctx, Some(unread), None, None)
            .await?;
        self.reset(src).await
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn change_label(
        &mut self,
        src: ScrollerSource,
        label: LocalLabelId,
        with_filter: Option<ReadFilter>,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        tracing::debug!("Changing label to `{label}`");
        let _ = self.task.take();
        self.task = self
            .source
            .write()
            .await
            .change_state(&ctx, with_filter, Some(label), None)
            .await?;
        self.reset(src).await
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn change_keywords(
        &mut self,
        src: ScrollerSource,
        keywords: SearchOptions,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        tracing::debug!("Changing search keywords");
        let _ = self.task.take();
        self.task = self
            .source
            .write()
            .await
            .change_state(&ctx, None, None, Some(keywords))
            .await?;
        self.reset(src).await
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn clear(
        &mut self,
        src: ScrollerSource,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
        tracing::info!("Clearing cursor for current label");
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let _ = self.task.take();
        self.task = self.source.write().await.clear(&ctx).await?;
        self.reset(src).await
    }

    #[tracing::instrument(skip_all, fields(src=%src))]
    async fn change_include(
        &mut self,
        src: ScrollerSource,
        include: IncludeSwitch,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
        if self.alternative_labels.supports_include_filter() {
            let label = self.include_to_label(include).await;
            self.change_label(src, label, None).await
        } else {
            Ok(ScrollerListUpdate::None(src).into())
        }
    }

    async fn total(
        source: &RwLock<S>,
        ctx: &Weak<MailUserContext>,
    ) -> Result<u64, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;

        source.read().await.all_total(&ctx).await
    }

    async fn seen(
        source: &RwLock<S>,
        ctx: &Weak<MailUserContext>,
    ) -> Result<u64, MailContextError> {
        let ctx = ctx.upgrade().ok_or(MailContextError::MissingContext)?;

        source.read().await.seen_total(&ctx).await
    }

    /// Return the number of elements that have been synced.
    async fn synced(
        source: &RwLock<S>,
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
            info!("We do not see the first page, requesting fetch more");
            Self::schedule_fetch_more(&self.ordered_command_send, src).await;
        }

        Ok(())
    }

    async fn sync_next(&mut self) -> Result<Vec<S::Item>, MailContextError> {
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

        debug!("Fetched next page, items number: {}", items.len());
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
        debug!("Awaiting for previous task");

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
            .send_async(ScrollerOrderedCommand::FetchMore { src, tx: None })
            .await
            .inspect_err(|e| tracing::error!("Failed to schedule fetch more command: {e:?}"));
    }

    async fn recalculate_alternative_labels(
        &mut self,
        label: LocalLabelId,
    ) -> Result<(), MailContextError> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::MissingContext)?;
        let tether = ctx.user_stash().connection().await?;
        let alternative_labels = AlternativeLabels::new(label, &tether).await?;
        self.alternative_labels = alternative_labels;

        Ok(())
    }

    async fn include_to_label(&self, include: IncludeSwitch) -> LocalLabelId {
        if self.alternative_labels.supports_include_filter() {
            match include {
                IncludeSwitch::Default => self.alternative_labels.label,
                IncludeSwitch::WithSpamAndTrash => self.alternative_labels.alt_label.unwrap(),
            }
        } else {
            self.alternative_labels.label
        }
    }

    async fn reset(
        &mut self,
        src: ScrollerSource,
    ) -> Result<ScrollerUpdate<S::Item>, MailContextError> {
        self.items.write().clear();
        self.fetch_more(src).await?;
        self.refresh(true, src).await
    }
}

struct ScrollerWorkerHandle<S>
where
    S: MailScrollerSource,
{
    command: flume::Sender<ScrollerCommand<S::Item>>,
    ordered_command: flume::Sender<ScrollerOrderedCommand>,
    updates: flume::Receiver<ScrollerUpdate<S::Item>>,
    handle: DropRemoveTableObserverHandle,
    aborts: Vec<AbortHandle>,
}

enum ScrollerCommand<T: MailScrollerItem> {
    GetTotal(oneshot::Sender<Result<u64, MailContextError>>),
    GetSeen(oneshot::Sender<Result<u64, MailContextError>>),
    GetSynced(oneshot::Sender<Result<u64, MailContextError>>),
    HasMore(oneshot::Sender<Result<bool, MailContextError>>),
    Cursor(Arc<MailScroller<T>>, T::Id, oneshot::Sender<MailCursor<T>>),
}

#[derive(Debug, Derivative)]
#[derivative(PartialEq)]
enum ScrollerOrderedCommand {
    FetchMore {
        src: ScrollerSource,
        #[derivative(PartialEq = "ignore")]
        tx: Option<oneshot::Sender<()>>,
    },
    FetchNew(ScrollerSource),
    Refresh(ScrollerSource),
    ForceRefresh(ScrollerSource),
    GetItems(ScrollerSource),
    ChangeFilter {
        src: ScrollerSource,
        unread: ReadFilter,
    },
    ChangeLabel {
        src: ScrollerSource,
        label: LocalLabelId,
    },
    ChangeInclude {
        #[derivative(PartialEq = "ignore")]
        tx: oneshot::Sender<LocalLabelId>,
        src: ScrollerSource,
        include: IncludeSwitch,
    },
    ChangeKeywords {
        src: ScrollerSource,
        #[derivative(PartialEq = "ignore")]
        keywords: SearchOptions,
    },
    Clear(ScrollerSource),
    AlternativeLabels(#[derivative(PartialEq = "ignore")] oneshot::Sender<AlternativeLabels>),
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

    debug!("Prefix count: {prefix_count}");

    if old.len() == new.len() && prefix_count == old.len() {
        debug!("No update required");
        return ScrollerListUpdate::None(src).into();
    } else if prefix_count == old.len() {
        let items = new[prefix_count..].to_vec();
        debug!("Append: items number: {}", items.len());
        return ScrollerListUpdate::Append { src, items }.into();
    }

    let suffix_count = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a.scroller_eq(b))
        .count();

    debug!("Suffix count: {suffix_count}");

    match (prefix_count, suffix_count) {
        (prefix_count, 0) => {
            let idx = prefix_count;
            let items = new[prefix_count..].to_vec();
            debug!("Replace from: {idx}, items number: {}", items.len());
            ScrollerListUpdate::ReplaceFrom { src, idx, items }.into()
        }
        (0, suffix_count) => {
            let idx = old.len().saturating_sub(suffix_count);
            let items = {
                let idx = new.len().saturating_sub(suffix_count);
                new[..idx].to_vec()
            };
            debug!("Replace before: {idx}, items number: {}", items.len());
            ScrollerListUpdate::ReplaceBefore { src, idx, items }.into()
        }
        (prefix_count, suffix_count) => {
            let from = prefix_count;
            let to = old.len().saturating_sub(suffix_count);
            let items = {
                let to = new.len().saturating_sub(suffix_count);
                new[from..to].to_vec()
            };
            debug!("Replace range: {from}..{to}, items number: {}", items.len());
            ScrollerListUpdate::ReplaceRange {
                src,
                from,
                to,
                items,
            }
            .into()
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl<T: Clone> Clone for ScrollerUpdate<T> {
    fn clone(&self) -> Self {
        match self {
            ScrollerUpdate::List(update) => match update {
                ScrollerListUpdate::None(src) => ScrollerListUpdate::None(*src).into(),
                ScrollerListUpdate::Append { src, items } => ScrollerListUpdate::Append {
                    src: *src,
                    items: items.clone(),
                }
                .into(),
                ScrollerListUpdate::ReplaceFrom { src, idx, items } => {
                    ScrollerListUpdate::ReplaceFrom {
                        src: *src,
                        idx: *idx,
                        items: items.clone(),
                    }
                    .into()
                }
                ScrollerListUpdate::ReplaceBefore { src, idx, items } => {
                    ScrollerListUpdate::ReplaceBefore {
                        src: *src,
                        idx: *idx,
                        items: items.clone(),
                    }
                    .into()
                }
                ScrollerListUpdate::ReplaceRange {
                    src,
                    from,
                    to,
                    items,
                } => ScrollerListUpdate::ReplaceRange {
                    src: *src,
                    from: *from,
                    to: *to,
                    items: items.clone(),
                }
                .into(),
            },
            ScrollerUpdate::Error { .. } => panic!("Cannot clone error update"),
            ScrollerUpdate::Status(update) => match update {
                ScrollerStatusUpdate::FetchNewStart(src) => {
                    ScrollerStatusUpdate::FetchNewStart(*src).into()
                }
                ScrollerStatusUpdate::FetchNewEnd(src) => {
                    ScrollerStatusUpdate::FetchNewEnd(*src).into()
                }
            },
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
    #[test_case(vec![], vec![] => matches ScrollerUpdate::List(ScrollerListUpdate::None(_)); "Test 1: empty to empty")]
    #[test_case(vec![], vec![1] => matches ScrollerUpdate::List(ScrollerListUpdate::Append { items, .. }) if items == vec![1]; "Test 2: empty to single item")]
    #[test_case(vec![], vec![1, 2, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::Append { items, .. }) if items == vec![1, 2, 3]; "Test 3: empty to multiple items")]
    #[test_case(vec![1], vec![] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 0, items, .. }) if items.is_empty(); "Test 4: single item to empty")]
    #[test_case(vec![1, 2, 3], vec![] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 0, items, .. }) if items.is_empty(); "Test 5: multiple items to empty")]
    #[test_case(vec![1], vec![1] => matches ScrollerUpdate::List(ScrollerListUpdate::None(_)); "Test 6: same single item")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::None(_)); "Test 7: same multiple items")]
    // Items added at the beginning
    #[test_case(vec![1, 2, 3], vec![0, 1, 2, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 0, items, .. }) if items == vec![0]; "Test 8: add one item at beginning")]
    #[test_case(vec![1, 2, 3], vec![0, -1, 1, 2, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 0, items, .. }) if items == vec![0, -1]; "Test 9: add two items at beginning")]
    #[test_case(vec![3, 4, 5], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 0, items, .. }) if items == vec![1, 2]; "Test 10: add items at beginning with all suffix common")]
    // Items added at the end
    #[test_case(vec![1, 2, 3], vec![1, 2, 3, 4] => matches ScrollerUpdate::List(ScrollerListUpdate::Append { items, .. }) if items == vec![4]; "Test 11: add one item at end")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::List(ScrollerListUpdate::Append { items, .. }) if items == vec![4, 5]; "Test 12: add two items at end")]
    // Items added in the middle
    #[test_case(vec![1, 3], vec![1, 2, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange { from: 1, to: 1, items, .. }) if items == vec![2]; "Test 13: add item in middle")]
    #[test_case(vec![1, 4], vec![1, 2, 3, 4] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange { from: 1, to: 1, items, .. }) if items == vec![2, 3]; "Test 14: add two items in middle")]
    #[test_case(vec![1, 4, 5], vec![1, 2, 3, 4, 5] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange { from: 1, to: 1, items, .. }) if items == vec![2, 3]; "Test 14a: add two items in middle")]
    // Items removed from beginning
    #[test_case(vec![1, 2, 3], vec![2, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 1, items, .. }) if items.is_empty(); "Test 15: remove one item from beginning")]
    #[test_case(vec![1, 2, 3, 4], vec![3, 4] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 2, items, .. }) if items.is_empty(); "Test 16: remove two items from beginning")]
    // Items removed from end
    #[test_case(vec![1, 2, 3], vec![1, 2] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 2, items, .. }) if items.is_empty(); "Test 17: remove one item from end")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 2] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 2, items, .. }) if items.is_empty(); "Test 18: remove two items from end")]
    // Items removed from middle
    #[test_case(vec![1, 2, 3], vec![1, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange { from: 1, to: 2, items, .. }) if items.is_empty(); "Test 19: remove item from middle")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 4] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange { from: 1, to: 3, items, .. }) if items.is_empty(); "Test 20: remove two items from middle")]
    #[test_case(vec![1, 2, 3, 4, 5], vec![1, 4, 5] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange { from: 1, to: 3, items, .. }) if items.is_empty(); "Test 20a: remove two items from middle")]
    // Items replaced
    #[test_case(vec![1, 2, 3], vec![1, 4, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange { from: 1, to: 2, items, .. }) if items == vec![4]; "Test 21: replace item in middle")]
    #[test_case(vec![1, 2, 3], vec![4, 2, 3] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 1, items, .. }) if items == vec![4]; "Test 22: replace first item")]
    #[test_case(vec![1, 2, 3], vec![1, 2, 4] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 2, items, .. }) if items == vec![4]; "Test 23: replace last item")]
    // Completely different vectors
    #[test_case(vec![1, 2, 3], vec![4, 5, 6] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 0, items, .. }) if items == vec![4, 5, 6]; "Test 24: completely different same length")]
    #[test_case(vec![1, 2], vec![3, 4, 5] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 0, items, .. }) if items == vec![3, 4, 5]; "Test 25: completely different new longer")]
    #[test_case(vec![1, 2, 3], vec![4, 5] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 0, items, .. }) if items == vec![4, 5]; "Test 26: completely different new shorter")]
    // Complex cases that test the algorithm's logic
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![0, 1, 2, 3, 4, 5, 6] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 0, items, .. }) if items == vec![0]; "Test 27: add at beginning with many common suffix")]
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![1, 2, 3, 7, 8, 9] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 3, items, .. }) if items == vec![7, 8, 9]; "Test 28: replace latter half")]
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![7, 8, 9, 4, 5, 6] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 3, items, .. }) if items == vec![7, 8, 9]; "Test 29: replace first half")]
    // Edge cases with single elements
    #[test_case(vec![1], vec![1, 2] => matches ScrollerUpdate::List(ScrollerListUpdate::Append { items, .. }) if items == vec![2]; "Test 30: single to two elements")]
    #[test_case(vec![1, 2], vec![1] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 1, items, .. }) if items.is_empty(); "Test 31: two to single element")]
    #[test_case(vec![1], vec![2] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 0, items, .. }) if items == vec![2]; "Test 32: single element replacement")]
    // Cases that test the 50% threshold logic
    #[test_case(vec![1, 2, 3, 4], vec![0, 2, 3, 4] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 1, items, .. }) if items == vec![0]; "Test 33: suffix common >= 50% triggers ReplaceBefore")]
    #[test_case(vec![1, 2, 3, 4], vec![1, 0, 0, 0] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx: 1, items, .. }) if items == vec![0, 0, 0]; "Test 34: prefix common > suffix common")]
    #[test_case(vec![1, 2, 3, 4, 5, 6], vec![0, 0, 0, 4, 5, 6] => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 3, items, .. }) if items == vec![0, 0, 0]; "Test 35: suffix wins over prefix")]
    // Large vectors to test performance characteristics
    #[test_case((1..=100).collect::<Vec<_>>(), (0..=100).collect::<Vec<_>>() => matches ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx: 0, items, .. }) if items == vec![0]; "Test 36: large vector add at beginning")]
    #[test_case((1..=100).collect::<Vec<_>>(), (1..=101).collect::<Vec<_>>() => matches ScrollerUpdate::List(ScrollerListUpdate::Append { items, .. }) if items == vec![101]; "Test 37: large vector add at end")]

    // Test the actual function
    fn test_calculate_scroller_update(old: Vec<i32>, new: Vec<i32>) -> ScrollerUpdate<i32> {
        let result = calculate_scroller_update(&old, &new, test_source());
        let actual = apply_scroller_update(old, &result);
        assert_eq!(actual, new);
        result
    }

    fn apply_scroller_update(mut current: Vec<i32>, update: &ScrollerUpdate<i32>) -> Vec<i32> {
        match update {
            ScrollerUpdate::List(update) => match update {
                ScrollerListUpdate::None(_) => current,
                ScrollerListUpdate::Append { items, .. } => {
                    current.extend(items.clone());
                    current
                }
                ScrollerListUpdate::ReplaceFrom { idx, items, .. } => {
                    current.splice(idx.., items.clone());
                    current
                }
                ScrollerListUpdate::ReplaceBefore { idx, items, .. } => {
                    current.splice(..idx, items.clone());
                    current
                }
                ScrollerListUpdate::ReplaceRange {
                    from, to, items, ..
                } => {
                    current.splice(from..to, items.clone());
                    current
                }
            },
            ScrollerUpdate::Error { .. } => current,
            ScrollerUpdate::Status(_) => current,
        }
    }

    #[test]
    fn test_scroller_source_is_preserved() {
        let src = ScrollerSource::Database;
        let result = calculate_scroller_update(&[1, 2], &[1, 2, 3], src);

        match result {
            ScrollerUpdate::List(ScrollerListUpdate::Append {
                src: result_src, ..
            }) => {
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
            ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx, items, .. }) => {
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
            ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom { idx, items, .. }) => {
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
            ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore { idx, items, .. }) => {
                assert_eq!(idx, 3); // Common suffix: [40, 50] starting at idx 3 in old
                assert_eq!(items, vec![5, 15, 20, 35]); // New items before the common suffix
            }
            _ => panic!("Expected ReplaceBefore variant for this scenario"),
        }
    }
}
