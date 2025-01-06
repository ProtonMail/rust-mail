use crate::datatypes::{ContextualConversation, ReadFilter};
use crate::models::{
    CachedConverstationScrollData, Conversation, ConversationLabel, ConversationScrollData, Label,
};
use crate::{AppError, MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::prelude::{
    GetConversationsOptions, GetConversationsResponse,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LabelId, LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use sqlite_watcher::watcher::TableObserver;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether, WatcherHandle};
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::oneshot::Receiver;
use tokio::sync::Mutex;
use tracing::debug;

// #[cfg(test)]
// #[path = "tests/mail_scroller/scroller.rs"]
// mod tests_scroller;

#[cfg(test)]
#[path = "tests/mail_scroller/conversation_scroller.rs"]
mod conversation_scroller;

type MailPaginatorInitResult<T> = Option<Receiver<Result<Vec<T>, MailContextError>>>;
//TODO: Watcher creation
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
    ) -> impl Future<Output = Result<(u64, MailPaginatorInitResult<Self::Item>), MailContextError>>;

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
        &self,
        ctx: &MailUserContext,
    ) -> impl Future<Output = Result<(Vec<Self::Item>, u64), MailContextError>>;

    fn watched_tables(&self) -> Vec<String>;
}

//TODO(testing):
// - sync till end, `has_more` reports nothing
// - After one sync, creating a new paginator with the same parameters should not refetch.
// - older elements do not show up in visible range
// - newer elements show up in visible range
/// Paginate over mail related items which implement [`MailScrollerSource`].
///
/// You should use [`has_more()`] to check if more data is available and [`fetch_more()`] to
/// retrieve the data from the server.
///
/// Whether the data is cached or always updated from the server, depends on the implementation
/// of [`MailScrollerSource`].
pub struct MailScroller<T: MailScrollerSource + 'static> {
    ctx: Arc<MailUserContext>,
    source: Arc<T>,
    total: u64,
    init_await: MailPaginatorInitResult<T::Item>,
}

pub struct MailScrollerWatcher<T: MailScrollerSource + 'static> {
    sender: flume::Sender<()>,
    source: Arc<T>,
}

impl<T: MailScrollerSource + 'static> TableObserver for MailScrollerWatcher<T> {
    fn tables(&self) -> Vec<String> {
        self.source.watched_tables()
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
        let (total, init_await) = source.initialize(&ctx).await?;
        let source = Arc::new(source);

        Ok(Self {
            ctx,
            total,
            source,
            init_await,
        })
    }

    pub fn watch(&self) -> Result<WatcherHandle, StashError> {
        self.ctx.user_stash().subscribe_to(|sender| {
            Box::new(MailScrollerWatcher {
                sender,
                source: Arc::clone(&self.source),
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

        Ok(dbg!(visible_items) < dbg!(self.total))
    }

    /// Fetch more data from the server.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be fecthed or saved.
    pub async fn fetch_more(&mut self) -> Result<Vec<T::Item>, MailContextError> {
        // If initialization is fetching something in the background, we wait
        // on that task to finish first.
        if let Some(wait_init) = self.init_await.take() {
            return wait_init.await.map_err(|_| {
                // very unlikely to occur, but just in case.
                MailContextError::Other(anyhow!("Failed to receive source data"))
            })?;
        }

        let (items, new_total) = self.source.sync_next(&self.ctx).await?;
        self.total = new_total;
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
    local_label_id: LocalId,
    unread: ReadFilter,
    page_size: usize,
    cached: Arc<Mutex<Option<CachedConverstationScrollData>>>,
}

impl MailConversationScrollerSource {
    pub fn new(local_label_id: LocalId, unread: ReadFilter, page_size: usize) -> Self {
        Self {
            local_label_id,
            unread,
            page_size,
            cached: Arc::new(Mutex::new(None)),
        }
    }
}

impl MailScrollerSource for MailConversationScrollerSource {
    type Item = ContextualConversation;

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(u64, MailPaginatorInitResult<Self::Item>), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;
        let session = ctx.session().clone();
        let unread = self.unread;

        dbg!(&label);

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
            self.cached = Arc::new(Mutex::new(Some(scroller)));

            return Ok((label.total(self.unread), None));
        }

        // No entry exist, which means we have not synced this label yet.
        let (sender, receiver) = tokio::sync::oneshot::channel();

        debug!("Paginating for the first time spawning sync task.");
        let local_label_id = label.local_id.unwrap();
        let remote_label_id = label.remote_id.clone().unwrap();
        let page_size = self.page_size;
        tokio::spawn(async move {
            //TODO: could just use a join handle?
            let r = Self::sync_first_page(
                &session,
                tether,
                local_label_id,
                remote_label_id,
                unread,
                page_size,
            )
            .await;
            drop(sender.send(r));
        });

        Ok((label.total(self.unread), Some(receiver)))
    }

    async fn visible_items(
        &self,
        ctx: &MailUserContext,
    ) -> Result<Vec<Self::Item>, MailContextError> {
        let tether = ctx.user_stash().connection();

        // If we paginated here before we return visible items from the cached scroller
        if let Some(ref scroller) = *self.cached.lock().await {
            return Ok(scroller.visible_elements(&tether).await?);
        }

        // Otherwise use the database one
        match ConversationScrollData::find_with_key(self.local_label_id, self.unread, &tether)
            .await?
        {
            Some(cp) => Ok(cp.visible_elements(&tether).await?),
            // If we have not synced there is nothing to find.
            None => Ok(vec![]),
        }
    }

    async fn visible_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();

        // If we paginated here before we return visible items from the cached scroller
        if let Some(ref scroller) = *self.cached.lock().await {
            return Ok(scroller.visible_element_count(&tether).await?);
        }

        // If we have not synced there is nothing to count.
        if let Some(cp) =
            ConversationScrollData::find_with_key(self.local_label_id, self.unread, &tether).await?
        {
            Ok(cp.visible_element_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    async fn all_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;

        Ok(label.total(self.unread))
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn sync_next(
        &self,
        ctx: &MailUserContext,
    ) -> Result<(Vec<Self::Item>, u64), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;

        // If we paginated here before and the cached scroller can load more items
        // return them instead of making the request
        if let Some(ref mut scroller) = *self.cached.lock().await {
            if scroller.has_more(&tether).await? {
                let items = scroller.fetch_more(&tether).await?;

                return Ok((items, label.total(self.unread)));
            }
        }

        // Invalidate cached scroller as it either reached the end or never has been instantiated.
        let _ = self.cached.lock().await.take();

        // Safeguard against init failing for some reason.
        let conversations = if let Some(cp) =
            ConversationScrollData::find_with_key(self.local_label_id, self.unread, &tether).await?
        {
            // Sync next data.
            Self::sync_next_page(
                ctx.session(),
                tether,
                label.local_id.unwrap(),
                label.remote_id.clone().unwrap(),
                cp.remote_conversation_id,
                cp.conversation_time,
                self.unread,
                self.page_size,
            )
            .await?
        } else {
            // Sync clean data.
            Self::sync_first_page(
                ctx.session(),
                tether,
                label.local_id.unwrap(),
                label.remote_id.clone().unwrap(),
                self.unread,
                self.page_size,
            )
            .await?
        };

        Ok((conversations, label.total(self.unread)))
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
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,local_label_id, remote_label_id))]
    async fn sync_first_page(
        session: &Session,
        mut tether: Tether,
        local_label_id: LocalId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Vec<ContextualConversation>, MailContextError> {
        debug!("Syncing first page");
        let response = session
            .api()
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                label_id: Some(remote_label_id.into()),
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
            &mut tether,
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
        local_label_id: LocalId,
        remote_label_id: LabelId,
        last_element_id: RemoteId,
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
                label_id: Some(remote_label_id.into()),
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
        local_label_id: LocalId,
        conversations: Vec<Conversation>,
    ) -> Vec<ContextualConversation> {
        conversations
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, local_label_id))
            .collect()
    }

    async fn save_conversations(
        local_label_id: LocalId,
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
        let remote_id = last.remote_id.clone().unwrap();
        let display_order = last.display_order;

        let conv_paginator = Self::update_scroller_data(
            local_label_id,
            remote_id,
            unread,
            context_time,
            display_order,
            &tx,
        )
        .await?;

        debug!(
            "New last element id={}, time={}, order={}",
            conv_paginator.remote_conversation_id, context_time, display_order
        );

        tx.commit().await?;

        Ok(())
    }

    async fn update_scroller_data(
        local_label_id: LocalId,
        remote_conv_id: RemoteId,
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
