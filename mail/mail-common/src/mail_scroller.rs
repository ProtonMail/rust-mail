use crate::datatypes::{ContextualConversation, ReadFilter, SearchOptions};
use crate::models::{
    CachedScrollData, Conversation, ConversationScrollData, Message, MessageCounters, MessageLabel,
    MessageScrollData, ScrollData, SearchScrollData,
};
use crate::{AppError, MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_api_core::services::proton::common::LabelId;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::common::{ConversationId, MessageId};
use proton_api_mail::services::proton::prelude::{
    GetConversationsOptions, GetConversationsResponse, GetMessagesOptions,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LocalLabelId, SystemLabel};
use proton_core_common::models::{Label, ModelExtension};
use sqlite_watcher::watcher::TableObserver;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether, WatcherHandle};
use std::cmp;
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::debug;

#[cfg(test)]
#[path = "tests/mail_scroller/message_scroller.rs"]
mod message_scroller;

#[cfg(test)]
#[path = "tests/mail_scroller/conversation_scroller.rs"]
mod conversation_scroller;

type MailPaginatorJoinHandle = Option<JoinHandle<Result<(), MailContextError>>>;
pub trait MailScrollerSource: Send + Sync {
    type Item: Send + 'static;

    /// Initialize the data source and retrieve up to `element_count` elements from the server.
    ///
    /// You can return an optional join handle that [`MailScroller`] will use on the first
    /// call to [`MailScroller::fetch_more()`] if you want to preload some data in
    /// a background task.
    ///
    /// # Errors
    ///
    /// Return errors if the initialization or setup failed.
    fn initialize(
        &mut self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<(u64, MailPaginatorJoinHandle), MailContextError>> + Send;

    /// Return the items that fall into range of the synced data.
    ///
    /// If some item is outside that range and known to us, it should not be included.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    fn visible_items(
        &self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<Vec<Self::Item>, MailContextError>> + Send;

    /// Return the total number of items that fall into range of the synced data.
    ///
    /// If some item is outside that range and known to us, it should not be included.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    fn visible_items_total(
        &self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<u64, MailContextError>> + Send;

    /// Return the total number of items that fall into range of the synced data.
    ///
    /// If some item is outside that range and known to us, it should not be included.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    fn all_items_total(
        &self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<u64, MailContextError>> + Send;

    /// Sync the next section of data from the remote source which should return up to
    /// `element_count` results.
    ///
    /// This method can await until the data is fetched and should return the
    /// new elements that are valid in this interval as well as the new total.
    ///
    /// # Errors
    ///
    /// Return error if something failed.
    fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> impl Future<
        Output = Result<(Vec<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError>,
    > + Send;

    fn watched_tables(&self) -> Vec<String>;
}

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

pub struct MailScrollerWatcher {
    sender: flume::Sender<()>,
    tables: Vec<String>,
}

impl TableObserver for MailScrollerWatcher {
    fn tables(&self) -> Vec<String> {
        self.tables.clone()
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for MailScrollerWatcher: {}", e);
            })
            .ok();
    }
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

    pub fn watch(&self) -> Result<WatcherHandle, StashError> {
        self.ctx.user_stash().subscribe_to(|sender| {
            Box::new(MailScrollerWatcher {
                sender,
                tables: self.source.watched_tables(),
            })
        })
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
        let visible_items = self.source.visible_items_total(&self.ctx).await?;
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
        if let Some(wait_task) = self.task.take() {
            let result = wait_task
                .await
                .map_err(|_| MailContextError::Other(anyhow!("Failed to receive source data")))
                .and_then(|res| res);

            if result.is_err() {
                // We failed to fetch next page in the background. This is not the end of the world,
                // `MailScrollerSource::sync_next` will return new task, log and keep going.

                tracing::error!("Failed to fetch next page in the background: {:?}", result);
            }
        }

        let (items, new_total, task) = self.source.sync_next(&self.ctx).await?;
        self.total = new_total;
        self.task = task;
        Ok(items)
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
}

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

pub trait RemoteSource: ScrollData + Send {
    fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> impl Future<Output = Result<MailPaginatorJoinHandle, MailContextError>> + Send;

    fn spawn_background_sync(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> impl Future<Output = Result<MailPaginatorJoinHandle, MailContextError>> + Send;
}

impl RemoteSource for ConversationScrollData {
    async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let session = ctx.session().clone();
        let mut tether = ctx.user_stash().connection();
        let handle = tokio::task::spawn(async move {
            RemoteConversationScrollerSource::sync_first_page(
                &session,
                &mut tether,
                local_label_id,
                remote_label_id,
                unread,
                page_size,
            )
            .await?;

            Ok(())
        });

        Ok(Some(handle))
    }

    async fn spawn_background_sync(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        RemoteConversationScrollerSource::spawn_background_sync(
            ctx,
            scroller,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
        )
        .await
    }
}

impl RemoteSource for MessageScrollData {
    async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let session = ctx.session().clone();
        let mut tether = ctx.user_stash().connection();
        let handle = tokio::task::spawn(async move {
            RemoteMessageScrollerSource::sync_first_page(
                &session,
                &mut tether,
                local_label_id,
                remote_label_id,
                unread,
                page_size,
            )
            .await?;

            Ok(())
        });

        Ok(Some(handle))
    }

    async fn spawn_background_sync(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        RemoteMessageScrollerSource::spawn_background_sync(
            ctx,
            scroller,
            local_label_id,
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
                self.initialized = true;
                scroller.visible_elements(&tether).await?
            };

            let should_not_load_more_from_remote =
                scroller.has_more_than_a_page(&tether).await? || total < self.page_size as u64;

            let task = if should_not_load_more_from_remote {
                None
            } else {
                let cp = scroller.scroll_data(&tether).await?;
                self.spawn_background_sync(ctx, &cp, label.remote_id.clone().unwrap())
                    .await?
            };

            Ok((items, total, task))
        } else if total > 0 {
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

/// Mail scroller implementation for [`Conversation`] on in a [`Label`].
///
/// The scroller keeps track of the last element returned by the server for the
/// selected label and read filter. This element is then used to fetch
/// new data from the server.
#[derive(Debug)]
struct RemoteConversationScrollerSource;

impl RemoteConversationScrollerSource {
    async fn spawn_background_sync(
        ctx: &MailUserContext,
        scroller: &ConversationScrollData,
        label_local_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let remote_id = scroller.remote_conversation_id.clone();
        let conversation_time = scroller.conversation_time;
        let session = ctx.session().clone();

        let task = Some(tokio::spawn(async move {
            let tether = stash.connection();

            Self::sync_next_page(
                &session,
                tether,
                label_local_id,
                remote_label_id,
                remote_id,
                conversation_time,
                unread,
                page_size,
            )
            .await?;

            Ok(())
        }));

        Ok(task)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,local_label_id, remote_label_id))]
    async fn sync_first_page(
        session: &Session,
        tether: &mut Tether,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Vec<ContextualConversation>, MailContextError> {
        debug!("Syncing first page");
        let response = session
            .api()
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                label_id: Some(remote_label_id),
                page_size: page_size as u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        debug!("Fetched {} elements", response.conversations.len());

        if response.conversations.is_empty() {
            return Ok(vec![]);
        }
        let context_time = Self::context_time(&response, unread);

        let mut conversations: Vec<Conversation> = response
            .conversations
            .into_iter()
            .map(|c| c.into())
            .collect();

        Self::save_conversations(
            local_label_id,
            &mut conversations,
            unread,
            context_time,
            tether,
        )
        .await?;

        Ok(Self::contextual_conversations(
            local_label_id,
            conversations,
        ))
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,local_label_id, remote_label_id))]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        session: &Session,
        mut tether: Tether,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        last_element_id: ConversationId,
        last_element_time: u64,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Vec<ContextualConversation>, MailContextError> {
        debug!("Syncing next page");
        let mut response = session
            .api()
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                // time == 0 breaks the api query.
                end: Some(last_element_time),
                end_id: Some(last_element_id.clone()),
                label_id: Some(remote_label_id),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        if !response.conversations.is_empty() {
            // Unless we are filtering, end id is always the first element in the returned
            // data, even if there is are no more elements.
            if response.conversations[0].id == last_element_id {
                response.conversations.remove(0);
            } else if response.conversations.len() > page_size {
                // sometimes we get more elements back than we need.
                response.conversations.pop();
            }
        }

        debug!("Fetched {} elements", response.conversations.len());

        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        let context_time = Self::context_time(&response, unread);

        let mut conversations: Vec<Conversation> = response
            .conversations
            .into_iter()
            .map(|c| c.into())
            .collect();

        Self::save_conversations(
            local_label_id,
            &mut conversations,
            unread,
            context_time,
            &mut tether,
        )
        .await?;

        Ok(Self::contextual_conversations(
            local_label_id,
            conversations,
        ))
    }

    fn context_time(response: &GetConversationsResponse, unread: ReadFilter) -> Option<u64> {
        if unread != ReadFilter::All {
            // When filtering conversations, we need to use the contextual time
            // perform the next page query or the data will not be displayed
            // correctly.
            // This contextual time also does not match the ConversationLabel.context_time
            // we use to display the query results. This means that the data
            // will change after it is written to the database.
            response.conversations.last()?.context_time
        } else {
            None
        }
    }

    fn contextual_conversations(
        local_label_id: LocalLabelId,
        conversations: Vec<Conversation>,
    ) -> Vec<ContextualConversation> {
        conversations
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, local_label_id))
            .collect()
    }

    async fn save_conversations(
        local_label_id: LocalLabelId,
        conversations: &mut [Conversation],
        unread: ReadFilter,
        context_time: Option<u64>,
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        let tx = tether.transaction().await?;

        // Save all conversations.
        for conversation in conversations.iter_mut() {
            conversation.save(&tx).await?
        }

        let Some((last, label)) = conversations
            .iter()
            .rev()
            .filter_map(|conv| {
                let conv_label = conv.label(local_label_id)?;
                Some((conv, conv_label))
            })
            .next()
        else {
            return Err(MailContextError::Other(anyhow!(
                "There is no conversation with labels"
            )));
        };

        let context_time = context_time.unwrap_or(label.context_time);
        // Unwrap safety: RemoteId is present as this method is called on conversation
        // downloaded from API
        let remote_id = last.remote_id.clone().unwrap();
        let display_order = last.display_order;

        Self::update_scroller_data(
            local_label_id,
            remote_id.clone(),
            unread,
            context_time,
            display_order,
            &tx,
        )
        .await?;

        debug!(
            "New last element id={:?}, time={}, order={}",
            remote_id, context_time, display_order
        );

        tx.commit().await?;

        Ok(())
    }

    async fn update_scroller_data(
        local_label_id: LocalLabelId,
        remote_conv_id: ConversationId,
        unread: ReadFilter,
        context_time: u64,
        display_order: u64,
        bond: &Bond<'_>,
    ) -> Result<ConversationScrollData, MailContextError> {
        let mut conv_paginator = ConversationScrollData::builder()
            .local_label_id(local_label_id)
            .unread(unread)
            .remote_conversation_id(remote_conv_id)
            .conversation_time(context_time)
            .display_order(display_order)
            .build();

        conv_paginator.save(bond).await?;

        Ok(conv_paginator)
    }
}

/// Mail scroller implementation for [`Message`] on in a [`Label`].
///
/// The scroller keeps track of the last element returned by the server for the
/// selected label and read filter. This element is then used to fetch
/// new data from the server.
#[derive(Debug)]
struct RemoteMessageScrollerSource;

impl RemoteMessageScrollerSource {
    async fn spawn_background_sync(
        ctx: &MailUserContext,
        scroller: &MessageScrollData,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let remote_id = scroller.remote_message_id.clone();
        let message_time = scroller.message_time;
        let session = ctx.session().clone();

        let task = Some(tokio::spawn(async move {
            let tether = stash.connection();

            Self::sync_next_page(
                &session,
                tether,
                local_label_id,
                remote_label_id,
                remote_id,
                message_time,
                unread,
                page_size,
            )
            .await?;

            Ok(())
        }));

        Ok(task)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,local_label_id, remote_label_id))]
    async fn sync_first_page(
        session: &Session,
        tether: &mut Tether,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Vec<Message>, MailContextError> {
        debug!("Syncing first page");
        let response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        debug!("Fetched {} elements", response.messages.len());

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, tether).await?);
        }

        Self::save_messages(local_label_id, &mut messages, unread, tether).await?;

        Ok(messages)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,local_label_id, remote_label_id))]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        session: &Session,
        mut tether: Tether,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        last_element_id: MessageId,
        last_element_time: u64,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Vec<Message>, MailContextError> {
        debug!("Syncing next page");
        let mut response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                // time == 0 breaks the api query.
                end: Some(last_element_time),
                end_id: Some(last_element_id.clone()),
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        if !response.messages.is_empty() {
            // Unless we are filtering, end id is always the first element in the returned
            // data, even if there is are no more elements.
            if response.messages[0].id == last_element_id {
                response.messages.remove(0);
            } else if response.messages.len() > page_size {
                // sometimes we get more elements back than we need.
                response.messages.pop();
            }
        }

        debug!("Fetched {} elements", response.messages.len());

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(local_label_id, &mut messages, unread, &mut tether).await?;

        Ok(messages)
    }

    async fn save_messages(
        local_label_id: LocalLabelId,
        messages: &mut [Message],
        unread: ReadFilter,
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        let tx = tether.transaction().await?;

        if messages.is_empty() {
            return Ok(());
        }

        // Save all conversations.
        for message in messages.iter_mut() {
            message.save(&tx).await?
        }

        let last = messages.last().unwrap();
        let time = last.time;
        // Unwrap safety: RemoteId is present as this method is called on message
        // downloaded from API
        let remote_id = last.remote_id.clone().unwrap();
        let display_order = last.display_order;

        Self::update_scroller_data(
            local_label_id,
            remote_id.clone(),
            unread,
            time,
            display_order,
            &tx,
        )
        .await?;

        debug!(
            "New last element id={:?}, time={}, order={}",
            remote_id, time, display_order
        );

        tx.commit().await?;

        Ok(())
    }

    async fn update_scroller_data(
        local_label_id: LocalLabelId,
        remote_msg_id: MessageId,
        unread: ReadFilter,
        time: u64,
        display_order: u64,
        bond: &Bond<'_>,
    ) -> Result<MessageScrollData, MailContextError> {
        let mut msg_paginator = MessageScrollData::builder()
            .local_label_id(local_label_id)
            .unread(unread)
            .remote_message_id(remote_msg_id)
            .message_time(time)
            .display_order(display_order)
            .build();

        msg_paginator.save(bond).await?;

        Ok(msg_paginator)
    }
}

/// Mail scroller implementation for Server search.
///
/// The scroller keeps track of the last element returned by the server for the
/// selected search query. This element is then used to fetch next pages
///
#[derive(Debug)]
pub struct SearchScrollerSource {
    search: SearchOptions,
    page_size: usize,
    initialized: bool,
    total: Arc<Mutex<u64>>,
    last: Option<SearchScrollData>,
}

impl SearchScrollerSource {
    pub fn new(search: SearchOptions, page_size: usize) -> Self {
        Self {
            search,
            page_size,
            initialized: false,
            total: Arc::new(Mutex::new(0)),
            last: None,
        }
    }

    async fn total(&self, tether: &Tether) -> Result<u64, StashError> {
        let total = *self.total.lock().await;
        Ok(match &self.last {
            Some(last) if last.has_more(tether).await? => cmp::max(
                total,
                last.visible_element_count(tether).await? + self.page_size as u64,
            ),
            Some(last) => cmp::max(total, last.visible_element_count(tether).await?),
            None => total,
        })
    }

    async fn spawn_first_page_sync(
        ctx: &MailUserContext,
        total: Arc<Mutex<u64>>,
        remote_label_id: LabelId,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let session = ctx.session().clone();

        let task = Some(tokio::spawn(async move {
            let mut tether = stash.connection();

            Self::sync_first_page(
                &session,
                &total,
                &mut tether,
                remote_label_id,
                search,
                page_size,
            )
            .await?;

            Ok(())
        }));

        Ok(task)
    }

    async fn spawn_background_sync(
        ctx: &MailUserContext,
        remote_label_id: LabelId,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let session = ctx.session().clone();

        let task = Some(tokio::spawn(async move {
            let tether = stash.connection();

            if let Some(remote_id) = SearchScrollData::last_remote_message_id(&tether).await? {
                Self::sync_next_page(
                    &session,
                    tether,
                    remote_label_id,
                    remote_id,
                    search,
                    page_size,
                )
                .await?;
            }

            Ok(())
        }));

        Ok(task)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,remote_label_id))]
    async fn sync_first_page(
        session: &Session,
        total: &Mutex<u64>,
        tether: &mut Tether,
        remote_label_id: LabelId,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<Vec<Message>, MailContextError> {
        debug!("Syncing first page");
        let response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64,
                keyword: search.keywords,
                ..Default::default()
            })
            .await?;
        let mut total = total.lock().await;
        *total = response.total;

        debug!(
            "Fetched {}/{} elements",
            response.messages.len(),
            response.total
        );

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, tether).await?);
        }

        Self::save_messages(&mut messages, tether).await?;

        Ok(messages)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,remote_label_id))]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        session: &Session,
        mut tether: Tether,
        remote_label_id: LabelId,
        last_element_id: MessageId,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<Vec<Message>, MailContextError> {
        debug!("Syncing next page");
        let mut response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                end_id: Some(last_element_id.clone()),
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64 + 1_u64,
                keyword: search.keywords,
                ..Default::default()
            })
            .await?;

        if !response.messages.is_empty() {
            // Unless we are filtering, end id is always the first element in the returned
            // data, even if there is are no more elements.
            if response.messages[0].id == last_element_id {
                response.messages.remove(0);
            } else if response.messages.len() > page_size {
                // sometimes we get more elements back than we need.
                response.messages.pop();
            }
        }

        debug!("Fetched {} elements", response.messages.len());

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(&mut messages, &mut tether).await?;

        Ok(messages)
    }

    async fn save_messages(
        messages: &mut [Message],
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        let tx = tether.transaction().await?;

        if messages.is_empty() {
            return Ok(());
        }

        let mut display_order = SearchScrollData::last(&tx)
            .await?
            .map(|s| s.display_order.saturating_add(1))
            .unwrap_or_default();

        // Save all messages.
        for message in messages.iter_mut() {
            message.save(&tx).await?;
            SearchScrollData::builder()
                .local_message_id(message.local_id.unwrap())
                .display_order(display_order)
                .build()
                .with_save(&tx)
                .await?;
            display_order = display_order.saturating_add(1);
        }

        let last = messages.last().unwrap();
        let time = last.time;
        // Unwrap safety: RemoteId is present as this method is called on message
        // downloaded from API
        let remote_id = last.remote_id.clone().unwrap();

        debug!(
            "New last element id={:?}, time={}, order={}",
            remote_id, time, display_order
        );

        tx.commit().await?;

        Ok(())
    }
}

impl MailScrollerSource for SearchScrollerSource {
    type Item = Message;

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(u64, MailPaginatorJoinHandle), MailContextError> {
        let mut tether = ctx.user_stash().connection();
        let bond = tether.transaction().await?;
        SearchScrollData::delete_all(&bond).await?;
        bond.commit().await?;

        // Search will always operate on online data only
        debug!("Paginating for the first time, getting first page & spawning sync task.");
        let remote_label_id = SystemLabel::AllMail.label_id();
        let page_size = self.page_size;

        // Wait for the first page to be fetched
        let task = Self::spawn_first_page_sync(
            ctx,
            self.total.clone(),
            remote_label_id.clone(),
            self.search.clone(),
            page_size,
        )
        .await?;

        Ok((page_size as u64 * 2, task))
    }

    async fn visible_items(
        &self,
        ctx: &MailUserContext,
    ) -> Result<Vec<Self::Item>, MailContextError> {
        let tether = ctx.user_stash().connection();
        if !self.initialized {
            Ok(vec![])
        } else if let Some(ref last) = self.last {
            Ok(last.visible_elements(&tether).await?)
        } else {
            Ok(vec![])
        }
    }

    async fn visible_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();

        if !self.initialized {
            Ok(0)
        } else if let Some(ref last) = self.last {
            Ok(last.visible_element_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    async fn all_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();

        Ok(self.total(&tether).await?)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(Vec<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection();

        if !self.initialized {
            self.last = SearchScrollData::last(&tether).await?;
        }

        if let Some(ref mut last) = self.last {
            let items = if self.initialized {
                last.fetch_more(self.page_size, &tether).await?
            } else {
                self.initialized = true;
                last.visible_elements(&tether).await?
            };

            let (task, total) = if items.is_empty() {
                (None, 0)
            } else {
                let total = self.total(&tether).await?;
                let task = Self::spawn_background_sync(
                    ctx,
                    SystemLabel::AllMail.label_id(),
                    self.search.clone(),
                    self.page_size,
                )
                .await?;

                (task, total)
            };

            Ok((items, total, task))
        } else {
            // Fallback for failing to initialize the scroller
            let (_, task) = self.initialize(ctx).await?;
            Ok((vec![], 0, task))
        }
    }

    fn watched_tables(&self) -> Vec<String> {
        vec![
            Message::table_name().to_owned(),
            MessageLabel::table_name().to_owned(),
            MessageCounters::table_name().to_owned(),
        ]
    }
}
