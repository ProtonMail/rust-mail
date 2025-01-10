use crate::datatypes::{ContextualConversation, ReadFilter};
use crate::models::{
    CachedConverstationScrollData, Conversation, ConversationLabel, ConversationScrollData, Label,
};
use crate::{AppError, MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_api_core::services::proton::common::LabelId;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::common::ConversationId;
use proton_api_mail::services::proton::prelude::{
    GetConversationsOptions, GetConversationsResponse,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelExtension;
use sqlite_watcher::watcher::TableObserver;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether, WatcherHandle};
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::debug;

// #[cfg(test)]
// #[path = "tests/mail_scroller/scroller.rs"]
// mod tests_scroller;

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
    ) -> impl Future<Output = Result<(u64, MailPaginatorJoinHandle), MailContextError>>;

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
    ) -> impl Future<Output = Result<Vec<Self::Item>, MailContextError>>;

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
    ) -> impl Future<Output = Result<u64, MailContextError>>;

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
    ) -> impl Future<Output = Result<u64, MailContextError>>;

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
    ) -> impl Future<Output = Result<(Vec<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError>>;

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
    // init_await: MailPaginatorInitResult<T::Item>,
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

impl<T: MailScrollerSource> MailScroller<T> {
    /// Create a new instance with the `source` and the maximum `element_count` of elements
    /// that should be retrieved from the server on each request.
    ///
    /// # Errors
    ///
    /// Returns error if something went wrong with initializing the data source.
    pub async fn new(ctx: Arc<MailUserContext>, mut source: T) -> Result<Self, MailContextError> {
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
        if let Some(wait_init) = self.task.take() {
            let result = wait_init
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

/// Mail scroller implementation for [`Conversation`] on in a [`Label`].
///
/// The scroller keeps track of the last element returned by the server for the
/// selected label and read filter. This element is then used to fetch
/// new data from the server.
#[derive(Debug)]
pub struct MailConversationScrollerSource {
    local_label_id: LocalLabelId,
    unread: ReadFilter,
    page_size: usize,
    initialized: bool,
    cached: Option<CachedConverstationScrollData>,
}

impl MailConversationScrollerSource {
    pub fn new(local_label_id: LocalLabelId, unread: ReadFilter, page_size: usize) -> Self {
        Self {
            local_label_id,
            unread,
            page_size,
            initialized: false,
            cached: None,
        }
    }
}

impl MailScrollerSource for MailConversationScrollerSource {
    type Item = ContextualConversation;

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(u64, MailPaginatorJoinHandle), MailContextError> {
        let mut tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;
        let session = ctx.session().clone();
        let unread = self.unread;

        // Check if we have a data suggesting we have synced this label before
        if let Some(scroller) = CachedConverstationScrollData::new(
            self.local_label_id,
            self.unread,
            self.page_size,
            &tether,
        )
        .await?
        {
            debug!("We have paginated here before, create cached scroller");
            let cp = scroller.data().clone();
            let task = if scroller.has_more_than_a_page(&tether).await?
                || label.total_conversations(self.unread) < self.page_size as u64
            {
                None
            } else {
                self.spawn_background_sync(ctx, &cp, label.remote_id.clone().unwrap())
                    .await?
            };

            self.cached = Some(scroller);
            self.initialized = true;

            return Ok((label.total_conversations(self.unread), task));
        }

        // No entry exist, which means we have not synced this label yet.
        debug!("Paginating for the first time, getting first page & spawning sync task.");
        let local_label_id = label.local_id.unwrap();
        let remote_label_id = label.remote_id.clone().unwrap();
        let page_size = self.page_size;

        // Wait for the first page to be fetched
        Self::sync_first_page(
            &session,
            &mut tether,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
        )
        .await?;

        // Sync the scroller data
        self.sync_scroller(&tether).await?;

        if let Some(ref scroller) = self.cached {
            // And spawn a background task to fetch the next page
            let cp = scroller.data();
            let task = if label.total_conversations(self.unread) < self.page_size as u64 {
                None
            } else {
                self.spawn_background_sync(ctx, cp, label.remote_id.clone().unwrap())
                    .await?
            };

            self.initialized = true;

            Ok((label.total_conversations(self.unread), task))
        } else if label.total_conversations(self.unread) == 0 {
            debug!("Empty label, no need to initialize scroller");
            // Do not initialize scroller as there might be an update to the label in the future.
            // But as there is nothing to paginate over, simply return an empty list.
            Ok((0, None))
        } else {
            // This might happen when there is no network to get the first page.
            // Initialization of the scroller failed, and it has to be done once more.
            // There is a fallback in the `fetch_more` method to handle this case.
            Err(MailContextError::Other(anyhow!(
                "Failed to initialize scroller"
            )))
        }
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
        if let Some(ref scroller) = self.cached {
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
        if let Some(ref scroller) = self.cached {
            Ok(scroller.visible_element_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    async fn all_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;

        Ok(label.total_conversations(self.unread))
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(Vec<Self::Item>, u64, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;

        // Fallback for failing to initialize the scroller
        if !self.initialized {
            debug!("Scroller not initialized, fallback to initialization");
            let (_, task) = self.initialize(ctx).await?;
            if let Some(ref scroller) = self.cached {
                let items = scroller.visible_elements(&tether).await?;
                return Ok((items, label.total_conversations(self.unread), task));
            } else {
                // In practice this branch should never happen
                // as the initialization will fail on empty cache.
                return Err(MailContextError::Other(anyhow!(
                    "Failed to initialize scroller"
                )));
            }
        }

        // Always sync the cache as there might be new data
        self.sync_scroller(&tether).await?;

        if let Some(ref mut scroller) = self.cached {
            // This is the only place where cache progresses,
            // There might be a case in which someone will try to fetch more
            // for the label which has no more data.
            // The the cache will not progress and `items` will be empty.
            // Note: Task is always spawned, if there is no more data to download.
            // As this information is provided in a trait. It is up to the implementation
            // To check if there is more data to download before asking for more.
            let items = scroller.fetch_more(&tether).await?;
            let cp = scroller.data().clone();

            let task = if scroller.has_more_than_a_page(&tether).await? {
                None
            } else {
                self.spawn_background_sync(ctx, &cp, label.remote_id.clone().unwrap())
                    .await?
            };

            Ok((items, label.total_conversations(self.unread), task))
        } else {
            // This is fallback for empty labels
            Ok((vec![], label.total_conversations(self.unread), None))
        }
    }

    fn watched_tables(&self) -> Vec<String> {
        vec![
            Conversation::table_name().to_string(),
            ConversationLabel::table_name().to_string(),
            Label::table_name().to_string(),
        ]
    }
}

impl MailConversationScrollerSource {
    async fn get_label(&self, tether: &Tether) -> Result<Label, MailContextError> {
        let Some(label) = Label::find_by_id(self.local_label_id, tether).await? else {
            return Err(AppError::LabelNotFound(self.local_label_id).into());
        };

        if label.remote_id.is_none() {
            return Err(AppError::LabelDoesNotHaveRemoteId(self.local_label_id).into());
        };

        Ok(label)
    }

    async fn sync_scroller(&mut self, tether: &Tether) -> Result<(), MailContextError> {
        if let Some(ref mut scroller) = self.cached {
            if !scroller.has_more_than_a_page(tether).await? {
                scroller.update(tether).await?;
            }
        } else {
            self.cached = CachedConverstationScrollData::new(
                self.local_label_id,
                self.unread,
                self.page_size,
                tether,
            )
            .await?;
        }

        Ok(())
    }

    async fn spawn_background_sync(
        &self,
        ctx: &MailUserContext,
        scroller: &ConversationScrollData,
        remote_label_id: LabelId,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let label_local_id = self.local_label_id;
        let unread = self.unread;
        let page_size = self.page_size;
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
