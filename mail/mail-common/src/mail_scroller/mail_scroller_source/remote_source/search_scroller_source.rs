use std::{cmp, sync::Arc};

use super::MailPaginatorJoinHandle;
use crate::datatypes::dependencies::MessageOrConversationDependencyFetcher;
use crate::datatypes::labels::ScrollOrderField;
use crate::{
    MailContextError, MailUserContext,
    datatypes::{ReadFilter, SearchOptions},
    mail_scroller::MailScrollerSource,
    models::{Message, MessageCounters, MessageLabel, SearchScrollData},
};
use proton_core_api::{services::proton::LabelId, session::Session};
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::{datatypes::SystemLabel, models::ModelExtension};
use proton_mail_api::services::proton::{
    ProtonMail, common::MessageId, prelude::GetMessagesOptions,
};
use stash::{
    orm::Model,
    stash::{StashError, Tether},
};
use tokio::sync::Mutex;
use tracing::debug;

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
    invalidate: Option<flume::Sender<()>>,
}

impl SearchScrollerSource {
    pub fn new(search: SearchOptions, page_size: usize) -> Self {
        Self {
            search,
            page_size,
            initialized: false,
            total: Arc::new(Mutex::new(0)),
            last: None,
            invalidate: None,
        }
    }

    async fn initialize_impl(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let mut tether = ctx.user_stash().connection().await?;
        tether
            .tx(async |tx| SearchScrollData::delete_all(tx).await)
            .await?;

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

        Ok(task)
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

        let task = Some(ctx.spawn(async move {
            let mut tether = stash.connection().await?;

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

        let task = Some(ctx.spawn(async move {
            let tether = stash.connection().await?;

            if let Some((remote_id, time)) =
                SearchScrollData::last_remote_message_id_and_time(&tether).await?
            {
                Self::sync_next_page(
                    &session,
                    tether,
                    remote_label_id,
                    remote_id,
                    time,
                    search,
                    page_size,
                )
                .await?;
            }

            Ok(())
        }));

        Ok(task)
    }

    #[tracing::instrument(skip_all, fields(label_id=?remote_label_id) )]
    async fn sync_first_page(
        session: &Session,
        total: &Mutex<u64>,
        tether: &mut Tether,
        remote_label_id: LabelId,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<Vec<Message>, MailContextError> {
        tracing::info!("Syncing first page in {remote_label_id:?}");

        let order_field = ScrollOrderField::for_label(&remote_label_id);

        let response = session
            .get_messages(GetMessagesOptions {
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64,
                keyword: search.keywords,
                desc: Some(true),
                sort: order_field.as_api_sort(),
                ..Default::default()
            })
            .await?;

        let mut total = total.lock().await;
        *total = response.total;
        drop(total);

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

        Self::save_messages(&mut messages, session, tether).await?;

        Ok(messages)
    }

    #[tracing::instrument(skip_all, fields(label_id=?remote_label_id) )]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        session: &Session,
        mut tether: Tether,
        remote_label_id: LabelId,
        last_element_id: MessageId,
        last_time: UnixTimestamp,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<Vec<Message>, MailContextError> {
        tracing::info!(
            "Syncing next page in {remote_label_id:?} with end_id={last_element_id:?} and end={last_time}"
        );
        let mut response = session
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                end: Some(last_time.as_u64()),
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
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(&mut messages, session, &mut tether).await?;

        Ok(messages)
    }

    async fn save_messages(
        messages: &mut [Message],
        api: &Session,
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        if messages.is_empty() {
            return Ok(());
        }

        // Resolve missing dependencies.
        let mut dependency_fetcher = MessageOrConversationDependencyFetcher::new();
        for message in messages.iter() {
            dependency_fetcher.check_message(message, tether).await?;
        }
        dependency_fetcher.fetch_and_store(api, tether).await?;
        // We do not want to notify the UI about the not visible items
        // downloaded in the background
        tether
            .quiet_tx(async |tx| {
                let mut display_order = SearchScrollData::last(tx)
                    .await?
                    .map(|s| s.display_order.saturating_add(1))
                    .unwrap_or_default();

                // Save all messages.
                for message in messages.iter_mut() {
                    message.create_or_get_local(tx).await?;
                    SearchScrollData::builder()
                        .local_message_id(message.id())
                        .display_order(display_order)
                        .build()
                        .with_save(tx)
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

                Ok(())
            })
            .await
    }
}

impl MailScrollerSource for SearchScrollerSource {
    type Item = Message;

    #[tracing::instrument(skip_all)]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
        invalidate: flume::Sender<()>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        self.invalidate = Some(invalidate);
        self.initialize_impl(ctx).await
    }

    async fn visible_items(
        &self,
        ctx: &MailUserContext,
    ) -> Result<Vec<Self::Item>, MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        if !self.initialized {
            Ok(vec![])
        } else if let Some(ref last) = self.last {
            Ok(last.visible_elements(&tether).await?)
        } else {
            Ok(vec![])
        }
    }

    async fn seen_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection().await?;

        if !self.initialized {
            Ok(0)
        } else if let Some(ref last) = self.last {
            Ok(last.visible_element_count(&tether).await?)
        } else {
            Ok(0)
        }
    }

    async fn synced_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        self.seen_total(ctx).await
    }

    async fn all_total(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
        let tether = ctx.user_stash().connection().await?;

        Ok(self.total(&tether).await?)
    }

    async fn has_more(&self, ctx: &MailUserContext) -> Result<bool, MailContextError> {
        let tether = ctx.user_stash().connection().await?;
        let has_more = match &self.last {
            Some(last) => last.has_more(&tether).await?,
            None => false,
        };

        Ok(has_more)
    }

    #[tracing::instrument(skip(ctx))]
    async fn sync_next(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<(Vec<Self::Item>, MailPaginatorJoinHandle), MailContextError> {
        let tether = ctx.user_stash().connection().await?;

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

            let task = if items.is_empty() {
                None
            } else {
                Self::spawn_background_sync(
                    ctx,
                    SystemLabel::AllMail.label_id(),
                    self.search.clone(),
                    self.page_size,
                )
                .await?
            };

            Ok((items, task))
        } else {
            Ok((vec![], None))
        }
    }

    async fn sync_new(
        &mut self,
        _ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        Ok(None)
    }

    async fn clear_cursor(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        self.initialize_impl(ctx).await
    }

    fn watched_tables(&self) -> Vec<String> {
        vec![
            Message::table_name().to_owned(),
            MessageLabel::table_name().to_owned(),
            MessageCounters::table_name().to_owned(),
        ]
    }

    async fn change_filter(
        &mut self,
        _ctx: &MailUserContext,
        _filter: ReadFilter,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        // Noop for search scroller
        Ok(None)
    }
}
