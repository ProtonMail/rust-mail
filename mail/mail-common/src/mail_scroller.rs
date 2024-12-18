use crate::datatypes::ContextualConversation;
use crate::models::{Conversation, ConversationScrollData, Label, ReadFilter};
use crate::{AppError, MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::prelude::GetConversationsOptions;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LabelId, LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use stash::stash::Tether;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::oneshot::Receiver;
use tracing::{debug, error, warn};

type MailPaginatorInitResult<T> = Option<Receiver<Result<Vec<T>, MailContextError>>>;
//TODO: Watcher creation
pub trait MailScrollerSource {
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
        &self,
        ctx: &MailUserContext,
        element_count: usize,
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
        element_count: usize,
    ) -> impl Future<Output = Result<(Vec<Self::Item>, u64), MailContextError>>;
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
pub struct MailScroller<T: MailScrollerSource> {
    ctx: Arc<MailUserContext>,
    source: T,
    total: u64,
    init_await: MailPaginatorInitResult<T::Item>,
    element_count: usize,
}

impl<T: MailScrollerSource> MailScroller<T> {
    /// Create a new instance with the `source` and the maximum `element_count` of elements
    /// that should be retrieved from the server on each request.
    ///
    /// # Errors
    ///
    /// Returns error if something went wrong with initializing the data source.
    pub async fn new(
        ctx: Arc<MailUserContext>,
        source: T,
        element_count: usize,
    ) -> Result<Self, MailContextError> {
        let (total, init_await) = source.initialize(&ctx, element_count).await?;

        Ok(Self {
            ctx,
            total,
            source,
            init_await,
            element_count,
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

        let (items, new_total) = self.source.sync_next(&self.ctx, self.element_count).await?;
        self.total = new_total;
        Ok(items)
    }

    /// Returns all the elements that are "visible" in the data source.
    ///
    /// # Errors
    ///
    /// Return error if the query failed.
    pub async fn all_items(&self) -> Result<Vec<T::Item>, MailContextError> {
        self.source.visible_items(&self.ctx).await
    }

    /// Return the total number of elements available.
    ///
    /// Note: This value does not react to changes until more
    /// data is fetched from the server.
    pub fn total(&self) -> u64 {
        //TODO: maybe consider querying the data source for the new total?
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
}
impl MailConversationScrollerSource {
    pub fn new(local_label_id: LocalId, unread: ReadFilter) -> Self {
        Self {
            local_label_id,
            unread,
        }
    }
}

impl MailScrollerSource for MailConversationScrollerSource {
    type Item = ContextualConversation;

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn initialize(
        &self,
        ctx: &MailUserContext,
        page_size: usize,
    ) -> Result<(u64, MailPaginatorInitResult<Self::Item>), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;
        let session = ctx.session().clone();
        let unread = self.unread;

        // Check if we have a data suggesting we have synced this label before
        if ConversationScrollData::find_with_key(self.local_label_id, self.unread, &tether)
            .await?
            .is_some()
        {
            debug!("We have paginated here before");
            return Ok((Self::label_total(self.unread, &label), None));
        }

        // No entry exist, which means we have not synced this label yet.
        let (sender, receiver) = tokio::sync::oneshot::channel();

        debug!("Paginating for the first time spawning sync task.");
        let local_label_id = label.local_id.unwrap();
        let remote_label_id = label.remote_id.clone().unwrap();
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

        Ok((Self::label_total(self.unread, &label), Some(receiver)))
    }

    async fn visible_items(
        &self,
        ctx: &MailUserContext,
    ) -> Result<Vec<Self::Item>, MailContextError> {
        let tether = ctx.user_stash().connection();

        // If we have not synced there is nothing to find.
        if let Some(cp) =
            ConversationScrollData::find_with_key(self.local_label_id, self.unread, &tether).await?
        {
            Ok(cp.visible_elements(&tether).await?)
        } else {
            Ok(vec![])
        }
    }

    async fn visible_items_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection();

        // If we have not synced there is nothing to count.
        if let Some(cp) =
            ConversationScrollData::find_with_key(self.local_label_id, self.unread, &tether).await?
        {
            Ok(cp.visible_element_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ctx))]
    async fn sync_next(
        &self,
        ctx: &MailUserContext,
        page_size: usize,
    ) -> Result<(Vec<Self::Item>, u64), MailContextError> {
        let tether = ctx.user_stash().connection();
        let label = self.get_label(&tether).await?;

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
                page_size,
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
                page_size,
            )
            .await?
        };

        Ok((conversations, Self::label_total(self.unread, &label)))
    }
}

impl MailConversationScrollerSource {
    //TODO: Move this to Label
    fn label_total(unread: ReadFilter, label: &Label) -> u64 {
        match unread {
            ReadFilter::All => label.total_conv,
            ReadFilter::Unread => label.unread_conv,
            ReadFilter::Read => label.total_conv.saturating_sub(label.unread_conv),
        }
    }
    async fn get_label(&self, tether: &Tether) -> Result<Label, MailContextError> {
        let Some(label) = Label::find_by_id(self.local_label_id, &tether).await? else {
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
        let context_time = if unread != ReadFilter::All {
            // When filtering conversations, we need to use the contextual time
            // perform the next page query or the data will not be displayed
            // correctly.
            // This contextual time also does not match the ConversationLabel.context_time
            // we use to display the query results. This means that the data
            // will change after it is written to the database.
            response
                .conversations
                .last()
                .expect("must be available")
                .context_time
        } else {
            None
        };

        let conversations: Vec<Conversation> = response
            .conversations
            .into_iter()
            .map(|c| c.into())
            .collect();

        Self::save_conversations(
            local_label_id,
            conversations,
            unread,
            context_time,
            &mut tether,
        )
        .await
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,tether,local_label_id, remote_label_id))]
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
                end_id: Some(last_element_id.clone().into()),
                label_id: Some(remote_label_id.into()),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        if !response.conversations.is_empty() {
            // Unless we are filtering, end id is always the first element in the returned
            // data, even if there is are no more elements.
            if response.conversations[0].id == last_element_id.into() {
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

        let context_time = if unread != ReadFilter::All {
            // When filtering conversations, we need to use the contextual time
            // perform the next page query or the data will not be displayed
            // correctly.
            // This contextual time also does not match the ConversationLabel.context_time
            // we use to display the query results. This means that the data
            // will change after it is written to the database.
            response
                .conversations
                .last()
                .expect("must be available")
                .context_time
        } else {
            None
        };

        let conversations: Vec<Conversation> = response
            .conversations
            .into_iter()
            .map(|c| c.into())
            .collect();

        Self::save_conversations(
            local_label_id,
            conversations,
            unread,
            context_time,
            &mut tether,
        )
        .await
    }

    async fn save_conversations(
        local_label_id: LocalId,
        mut conversations: Vec<Conversation>,
        unread: ReadFilter,
        context_time: Option<u64>,
        tether: &mut Tether,
    ) -> Result<Vec<ContextualConversation>, MailContextError> {
        let tx = tether.transaction().await?;

        // Save all conversations.
        for conversation in &mut conversations {
            conversation.save(&tx).await?
        }

        let conversations = conversations
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, local_label_id))
            .collect::<Vec<_>>();

        // Update Page data
        let (last_id, last_time, last_order) = {
            let last = conversations.last().unwrap();
            (
                last.remote_id.clone().unwrap(),
                last.time,
                last.display_order,
            )
        };

        let mut conv_paginator = ConversationScrollData {
            local_label_id,
            unread,
            remote_conversation_id: last_id,
            conversation_time: context_time.unwrap_or(last_time),
            display_order: last_order,
            row_id: None,
        };

        conv_paginator.save(&tx).await?;

        debug!(
            "New last element id={}, time={}, order={}",
            conv_paginator.remote_conversation_id, last_time, last_order
        );

        tx.commit().await?;

        Ok(conversations)
    }
}
