use super::MailPaginatorJoinHandle;
use crate::AppError;
use crate::datatypes::dependencies::DependencyFetcher;
use crate::datatypes::labels::ScrollOrderField;
use crate::models::MailBusyLabel;
use crate::{
    MailContextError, MailUserContext,
    datatypes::{ReadFilter, SearchOptions},
    mail_scroller::MailScrollerSource,
    models::{Message, MessageCounter, MessageLabel, SearchScrollData},
};
use proton_action_queue::queue::Queue;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::{services::proton::LabelId, session::Session};
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::{
    ProtonMail, common::MessageId, prelude::GetMessagesOptions,
    response_data::MessageMetadata as ApiMessageMetadata,
};
use stash::UserDb;
use stash::{
    orm::Model,
    stash::{StashError, Tether},
};
use std::{cmp, sync::Arc};
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument};

#[derive(Debug)]
pub struct SearchScrollerSource {
    local_label_id: LocalLabelId,
    options: SearchOptions,
    page_size: usize,
    initialized: bool,
    total: Arc<Mutex<u64>>,
    last: Option<SearchScrollData>,
    invalidate: Option<flume::Sender<()>>,
}

impl SearchScrollerSource {
    pub fn new(remote_label_id: LocalLabelId, options: SearchOptions, page_size: usize) -> Self {
        Self {
            local_label_id: remote_label_id,
            options,
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

        let Some(remote_label_id) =
            Label::local_id_counterpart(self.local_label_id, &tether).await?
        else {
            return Err(AppError::LabelDoesNotHaveRemoteId(self.local_label_id).into());
        };

        debug!("Paginating for the first time, getting first page & spawning sync task.");

        Self::spawn_first_page_sync(
            ctx,
            self.total.clone(),
            remote_label_id,
            self.options.clone(),
            self.page_size,
        )
        .await
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

    pub(crate) async fn spawn_first_page_sync(
        ctx: &MailUserContext,
        total: Arc<Mutex<u64>>,
        remote_label_id: LabelId,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let session = ctx.session().clone();

        let task = ctx.spawn_ex(async move |ctx| {
            let mut tether = stash.connection().await?;

            Self::sync_first_page(
                &session,
                &total,
                &mut tether,
                remote_label_id,
                search,
                page_size,
                ctx.action_queue(),
            )
            .await?;

            Ok(())
        });

        Ok(Some(task))
    }

    pub(crate) async fn spawn_background_sync(
        ctx: &MailUserContext,
        remote_label_id: LabelId,
        search: SearchOptions,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let session = ctx.session().clone();

        let task = ctx.spawn_ex(async move |ctx| {
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
                    ctx.action_queue(),
                )
                .await?;
            }

            Ok(())
        });

        Ok(Some(task))
    }

    #[instrument(skip_all)]
    async fn sync_first_page(
        session: &Session,
        total: &Mutex<u64>,
        tether: &mut Tether,
        remote_label_id: LabelId,
        search: SearchOptions,
        page_size: usize,
        queue: &Queue<UserDb>,
    ) -> Result<Vec<Message>, MailContextError> {
        info!("Syncing first page in {remote_label_id:?}");

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

        Self::save_messages(response.messages, session, tether, queue).await
    }

    #[instrument(skip_all)]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        session: &Session,
        mut tether: Tether,
        remote_label_id: LabelId,
        last_element_id: MessageId,
        last_time: UnixTimestamp,
        search: SearchOptions,
        page_size: usize,
        queue: &Queue<UserDb>,
    ) -> Result<Vec<Message>, MailContextError> {
        info!(
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

        Self::save_messages(response.messages, session, &mut tether, queue).await
    }

    async fn save_messages(
        api_messages: Vec<ApiMessageMetadata>,
        api: &Session,
        tether: &mut Tether,
        queue: &Queue<UserDb>,
    ) -> Result<Vec<Message>, MailContextError> {
        if api_messages.is_empty() {
            return Ok(vec![]);
        }

        // Resolve missing dependencies.
        let mut dependency_fetcher = DependencyFetcher::new();
        for message in api_messages.iter() {
            dependency_fetcher
                .check_api_message_metadata(message, tether)
                .await?;
        }
        let unresolved_label_ids = dependency_fetcher.fetch_and_store(api, tether).await?;
        // We do not want to notify the UI about the not visible items
        // downloaded in the background

        tether
            .quiet_tx(async |tx| {
                let mut rebase_change_set = RebaseChangeSet::default();
                let mut display_order = SearchScrollData::last(tx)
                    .await?
                    .map(|s| s.display_order.saturating_add(1))
                    .unwrap_or_default();

                let mut messages = Message::save_scroller_messages(
                    api_messages,
                    &mut rebase_change_set,
                    &unresolved_label_ids,
                    tx,
                )
                .await?;
                for message in messages.iter_mut() {
                    SearchScrollData::builder()
                        .local_message_id(message.id())
                        .display_order(display_order)
                        .build()
                        .with_save(tx)
                        .await?;
                    display_order = display_order.saturating_add(1);
                }

                if let Err(e) = queue
                    .rebase_in(
                        proton_action_queue::action::ActionGroup::default(),
                        &rebase_change_set,
                        tx,
                    )
                    .await
                {
                    error!("Failed to rebase: {e}");
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

                Ok(messages)
            })
            .await
    }
}

impl MailScrollerSource for SearchScrollerSource {
    type Item = Message;

    #[instrument(skip_all)]
    async fn initialize(
        &mut self,
        ctx: &MailUserContext,
        invalidate: flume::Sender<()>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        self.invalidate = Some(invalidate);
        self.initialize_impl(ctx).await
    }

    async fn visible_elements(
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

    async fn seen_count(&self, ctx: &MailUserContext) -> Result<u64, MailContextError> {
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
        self.seen_count(ctx).await
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

    #[instrument(skip(ctx))]
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
                let Some(remote_label_id) =
                    Label::local_id_counterpart(self.local_label_id, &tether).await?
                else {
                    return Err(AppError::LabelDoesNotHaveRemoteId(self.local_label_id).into());
                };

                Self::spawn_background_sync(
                    ctx,
                    remote_label_id,
                    self.options.clone(),
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

    async fn clear(
        &mut self,
        ctx: &MailUserContext,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        self.initialize_impl(ctx).await
    }

    fn watched_tables(&self) -> Vec<String> {
        vec![
            Message::table_name().to_owned(),
            MessageLabel::table_name().to_owned(),
            MessageCounter::table_name().to_owned(),
            MailBusyLabel::table_name().to_owned(),
        ]
    }

    async fn change_state(
        &mut self,
        ctx: &MailUserContext,
        _unread: Option<ReadFilter>,
        label: Option<LocalLabelId>,
        keywords: Option<SearchOptions>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        if let Some(label) = label {
            info!(
                "Changing label from {current:?} to {label:?}",
                current = self.local_label_id
            );
            self.local_label_id = label;
        }

        if let Some(keywords) = keywords {
            info!("Changing search parameters");
            self.options = keywords;
        }

        self.initialized = false;
        self.last = None;
        let task = self.initialize_impl(ctx).await?;

        Ok(task)
    }
}
