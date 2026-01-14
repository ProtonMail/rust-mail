use super::{MailPaginatorJoinHandle, RemoteSource, utils};
use crate::datatypes::MessageLabelsCount;
use crate::datatypes::dependencies::MessageOrConversationDependencyFetcher;
use crate::datatypes::labels::ScrollOrderDir;
use crate::datatypes::labels::ScrollOrderField;
use crate::models::LabelExt;
use crate::prefetch::PrefetchJob;
use crate::{
    MailContextError, MailUserContext,
    datatypes::ReadFilter,
    models::{Message, MessageScrollData},
};
use anyhow::anyhow;
use itertools::Itertools;
use proton_action_queue::action::ActionGroup;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::{services::proton::LabelId, session::Session};
use proton_core_common::RebasableQueue;
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_core_common::models::Label;
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::{
    ProtonMail, common::MessageId, prelude::GetMessagesOptions, prelude::GetMessagesResponse,
    response_data::MessageMetadata as ApiMessageMetadata,
};
use stash::stash::{Bond, Tether};
use std::ops::ControlFlow;
use tracing::{debug, error, info, instrument};

#[derive(Debug)]
pub(super) struct RemoteMessageScrollerSource;

impl RemoteSource for MessageScrollData {
    fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        invalidate: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let handle = ctx.spawn_ex(async move |ctx| {
            let items = RemoteMessageScrollerSource::sync_first_page(
                &ctx,
                local_label_id,
                remote_label_id.clone(),
                unread,
                page_size,
                order_dir,
                order_field,
            )
            .await?;

            if let Some(invalidate) = invalidate
                && !items.is_empty()
            {
                invalidate.send_async(()).await.map_err(|e| {
                    MailContextError::Other(anyhow!(
                        "Could not notify about fetching first page: {e}"
                    ))
                })?;
            }

            let prefetch_jobs = items
                .into_iter()
                .filter(|item| !item.deleted)
                .filter_map(|item| Some(PrefetchJob::Message(item.local_id?)))
                .collect();

            ctx.queue_prefetch_jobs(prefetch_jobs).await?;

            Ok(())
        });

        Ok(Some(handle))
    }

    fn sync_next_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        RemoteMessageScrollerSource::spawn_background_sync(
            ctx,
            scroller,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
            order_dir,
            order_field,
        )
    }

    fn sync_previous_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        sender: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let remote_id = scroller.remote_message_id.clone();
        let context_time = scroller.context_time(order_field);

        let task = ctx.spawn_ex(async move |ctx| {
            let items = RemoteMessageScrollerSource::sync_previous_page(
                &ctx,
                local_label_id,
                remote_label_id,
                remote_id,
                context_time,
                unread,
                page_size,
                order_dir,
                order_field,
            )
            .await?;

            if let Some(sender) = sender
                && !items.is_empty()
            {
                sender.send_async(()).await.map_err(|e| {
                    MailContextError::Other(anyhow!(
                        "Could not notify about fetching previous page: {e}"
                    ))
                })?;
            }

            Ok(())
        });

        Ok(Some(task))
    }
}

impl RemoteMessageScrollerSource {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn spawn_background_sync(
        ctx: &MailUserContext,
        scroller: &MessageScrollData,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let remote_id = scroller.remote_message_id.clone();
        let context_time = scroller.context_time(order_field);

        let task = ctx.spawn_ex(async move |ctx| {
            Self::sync_next_page(
                &ctx,
                local_label_id,
                remote_label_id,
                remote_id,
                context_time,
                unread,
                page_size,
                order_dir,
                order_field,
            )
            .await?;

            Ok(())
        });

        Ok(Some(task))
    }

    #[instrument(skip_all)]
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<Vec<Message>, MailContextError> {
        info!(
            ?local_label_id,
            ?remote_label_id,
            ?unread,
            ?page_size,
            ?order_dir,
            ?order_field,
            "Syncing first page"
        );

        let response = ctx
            .session()
            .get_messages(GetMessagesOptions {
                label_id: Some(vec![remote_label_id.clone()]),
                page_size: page_size as u64,
                unread: unread.into(),
                desc: order_dir.as_api_desc(),
                sort: order_field.as_api_sort(),
                ..Default::default()
            })
            .await?;

        log_response(&response);

        let mut tether = ctx.user_stash().connection().await?;

        // ---

        let ControlFlow::Continue(()) = utils::ensure_label_is_idle(
            &mut tether,
            local_label_id,
            &remote_label_id,
            &response.tasks_running,
        )
        .await?
        else {
            return Ok(vec![]);
        };

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        // ---

        Self::save_messages(
            local_label_id,
            response.messages,
            unread,
            true,
            order_dir,
            order_field,
            vec![],
            ctx.session(),
            &mut tether,
            ctx.rebaseable_queue().await,
        )
        .await
    }

    #[instrument(skip_all)]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        last_element_id: MessageId,
        last_element_time: UnixTimestamp,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<Vec<Message>, MailContextError> {
        info!(
            ?local_label_id,
            ?remote_label_id,
            ?unread,
            ?page_size,
            ?order_dir,
            ?order_field,
            "Syncing next page"
        );

        let mut response = ctx
            .session()
            .get_messages(GetMessagesOptions {
                anchor: Some(last_element_time.as_u64()),
                anchor_id: Some(last_element_id.clone()),
                label_id: Some(vec![remote_label_id.clone()]),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                desc: order_dir.as_api_desc(),
                sort: order_field.as_api_sort(),
                ..Default::default()
            })
            .await?;

        log_response(&response);

        let mut tether = ctx.user_stash().connection().await?;

        // ---

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

        let ControlFlow::Continue(()) = utils::ensure_label_is_idle(
            &mut tether,
            local_label_id,
            &remote_label_id,
            &response.tasks_running,
        )
        .await?
        else {
            return Ok(vec![]);
        };

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        // ---

        Self::save_messages(
            local_label_id,
            response.messages,
            unread,
            true,
            order_dir,
            order_field,
            vec![],
            ctx.session(),
            &mut tether,
            ctx.rebaseable_queue().await,
        )
        .await
    }

    #[instrument(skip_all)]
    #[allow(clippy::too_many_arguments)]
    async fn sync_previous_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        first_element_id: MessageId,
        first_element_time: UnixTimestamp,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<Vec<Message>, MailContextError> {
        info!(
            ?local_label_id,
            ?remote_label_id,
            ?unread,
            ?page_size,
            ?order_dir,
            ?order_field,
            "Syncing previous page"
        );

        let response = ctx
            .session()
            .get_messages(GetMessagesOptions {
                anchor: Some(first_element_time.as_u64()),
                anchor_id: Some(first_element_id.clone()),
                label_id: Some(vec![remote_label_id.clone()]),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                desc: order_dir.reverse().as_api_desc(),
                sort: order_field.as_api_sort(),
                ..Default::default()
            })
            .await?;

        log_response(&response);

        let mut tether = ctx.user_stash().connection().await?;

        // ---

        let ControlFlow::Continue(()) = utils::ensure_label_is_idle(
            &mut tether,
            local_label_id,
            &remote_label_id,
            &response.tasks_running,
        )
        .await?
        else {
            return Ok(vec![]);
        };

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        // ---

        let message_label_counts = ctx
            .session()
            .get_messages_count_for_labels(vec![remote_label_id.clone()])
            .await?;

        let messages = Self::save_messages(
            local_label_id,
            response.messages,
            unread,
            false,
            order_dir,
            order_field,
            message_label_counts.counts.into_iter().map_into().collect(),
            ctx.session(),
            &mut tether,
            ctx.rebaseable_queue().await,
        )
        .await?;

        Ok(messages)
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_messages(
        local_label_id: LocalLabelId,
        api_messages: Vec<ApiMessageMetadata>,
        unread: ReadFilter,
        update_scroller: bool,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        message_labels_count: Vec<MessageLabelsCount>,
        api: &Session,
        tether: &mut Tether,
        queue: RebasableQueue<'_>,
    ) -> Result<Vec<Message>, MailContextError> {
        // Resolve missing dependencies.
        let mut dependency_fetcher = MessageOrConversationDependencyFetcher::new();
        for message in api_messages.iter() {
            dependency_fetcher
                .check_api_message_metadata(message, tether)
                .await?;
        }
        let unresolved_label_ids = dependency_fetcher.fetch_and_store(api, tether).await?;

        // We do not want to notify the UI about the not visible items
        // downloaded in the background
        let messages = tether
            .quiet_tx(async |tx| {
                if let Some(label) = Label::find_by_id(local_label_id, tx).await?
                    && label.is_busy(tx).await?
                {
                    return Ok(vec![]);
                }

                if !message_labels_count.is_empty() {
                    MessageLabelsCount::upsert(message_labels_count.clone(), tx).await?;
                }

                // Save all messages.
                let mut rebase_change_set = RebaseChangeSet::default();

                let messages = Message::save_scroller_messages(
                    api_messages,
                    &mut rebase_change_set,
                    queue.is_rebase_enabled(),
                    &unresolved_label_ids,
                    tx,
                )
                .await?;

                // We don't want this to cause failures in the scroller.
                if let Err(e) = queue
                    .rebase_in(ActionGroup::default(), &rebase_change_set, tx)
                    .await
                {
                    error!("Failed to rebase changes: {e}")
                }

                let last = messages.last().unwrap();
                let time = last.time;
                let snooze_time = last.snooze_time;

                // Unwrap safety: RemoteId is present as this method is called on message
                // downloaded from API
                let remote_id = last.remote_id.clone().unwrap();
                let display_order = last.display_order;

                if update_scroller {
                    Self::update_scroller_data(
                        local_label_id,
                        remote_id.clone(),
                        unread,
                        time,
                        snooze_time,
                        display_order,
                        order_dir,
                        order_field,
                        tx,
                    )
                    .await?;
                }

                Ok::<_, MailContextError>(messages)
            })
            .await?;

        //TODO(ET-5589): This should not be necessary
        // Fake save the counters here again they trigger db watcher updates.
        if !message_labels_count.is_empty()
            && let Err(e) = tether
                .tx(async |tx| MessageLabelsCount::fake_update(local_label_id, tx).await)
                .await
        {
            error!("Failed to trigger fake label counters update: {e}");
        }

        Ok(messages)
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_scroller_data(
        local_label_id: LocalLabelId,
        remote_msg_id: MessageId,
        unread: ReadFilter,
        time: UnixTimestamp,
        snooze_time: UnixTimestamp,
        display_order: u64,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        bond: &Bond<'_>,
    ) -> Result<MessageScrollData, MailContextError> {
        debug!(
            "New message cursor {remote_msg_id} at time={time}, snooze_time={snooze_time}, order={display_order}"
        );

        let mut msg_paginator = MessageScrollData::builder()
            .local_label_id(local_label_id)
            .unread(unread)
            .remote_message_id(remote_msg_id)
            .message_time(time)
            .snooze_time(snooze_time)
            .display_order(display_order)
            .order_dir(order_dir)
            .order_field(order_field)
            .build();

        msg_paginator.save(bond).await?;

        Ok(msg_paginator)
    }
}

fn log_response(response: &GetMessagesResponse) {
    debug!(
        "Fetched {}/{} {} elements",
        response.messages.len(),
        response.total,
        if response.stale { "stale" } else { "fresh" }
    );
}
