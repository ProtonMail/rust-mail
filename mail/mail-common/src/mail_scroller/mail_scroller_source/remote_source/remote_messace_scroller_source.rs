use anyhow::anyhow;
use proton_core_api::{
    services::proton::LabelId,
    session::{CoreSession, Session},
};
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_mail_api::services::proton::{
    ProtonMail, common::MessageId, prelude::GetMessagesOptions,
};
use stash::stash::{Bond, Stash, Tether};
use tracing::debug;

use super::{MailPaginatorJoinHandle, RemoteSource};
use crate::{
    MailContextError, MailUserContext,
    datatypes::ReadFilter,
    models::{Message, MessageScrollData},
};
use crate::{datatypes::labels::LabelScrollOrder, prefetch::PrefetchJob};

/// Mail scroller implementation for [`Message`] on in a [`Label`].
///
/// The scroller keeps track of the last element returned by the server for the
/// selected label and read filter. This element is then used to fetch
/// new data from the server.
#[derive(Debug)]
pub(super) struct RemoteMessageScrollerSource;

impl RemoteSource for MessageScrollData {
    async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let session = ctx.session().clone();
        let stash = ctx.user_stash().clone();
        let arc_ctx = ctx.as_arc();
        let handle = ctx.spawn(async move {
            let items = RemoteMessageScrollerSource::sync_first_page(
                &session,
                stash,
                local_label_id,
                remote_label_id,
                unread,
                page_size,
                scroll_order,
            )
            .await?;

            let prefetch_jobs = items
                .into_iter()
                .filter_map(|item| Some(PrefetchJob::Message(item.local_id?)))
                .collect();

            arc_ctx.queue_prefetch_jobs(prefetch_jobs).await?;

            Ok(())
        });

        Ok(Some(handle))
    }

    async fn sync_next_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        RemoteMessageScrollerSource::spawn_background_sync(
            ctx,
            scroller,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
            scroll_order,
        )
        .await
    }

    async fn sync_previous_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
        sender: flume::Sender<()>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let remote_id = scroller.remote_message_id.clone();
        let message_time = scroller.message_time;
        let session = ctx.session().clone();

        let task = Some(ctx.spawn(async move {
            let items = RemoteMessageScrollerSource::sync_previous_page(
                &session,
                stash,
                local_label_id,
                remote_label_id,
                remote_id,
                message_time,
                unread,
                page_size,
                scroll_order,
            )
            .await?;

            if !items.is_empty() {
                sender.send_async(()).await.map_err(|e| {
                    MailContextError::Other(anyhow!(
                        "Could not notify about fetching previous page: {e}"
                    ))
                })?;
            }

            Ok(())
        }));

        Ok(task)
    }
}

impl RemoteMessageScrollerSource {
    pub(super) async fn spawn_background_sync(
        ctx: &MailUserContext,
        scroller: &MessageScrollData,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let remote_id = scroller.remote_message_id.clone();
        let message_time = scroller.message_time;
        let session = ctx.session().clone();

        let task = Some(ctx.spawn(async move {
            Self::sync_next_page(
                &session,
                stash,
                local_label_id,
                remote_label_id,
                remote_id,
                message_time,
                unread,
                page_size,
                scroll_order,
            )
            .await?;

            Ok(())
        }));

        Ok(task)
    }

    #[tracing::instrument(skip_all, fields(label_id=local_label_id.as_u64(), unread=?unread) )]
    pub(super) async fn sync_first_page(
        session: &Session,
        stash: Stash,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Result<Vec<Message>, MailContextError> {
        tracing::info!("Syncing first page in {remote_label_id:?}");
        let response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(scroll_order == LabelScrollOrder::Descending),
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {}/{} elements",
            response.messages.len(),
            response.total
        );

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];
        let mut tether = stash.connection();

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(
            local_label_id,
            &mut messages,
            unread,
            true,
            scroll_order,
            &mut tether,
        )
        .await?;

        Ok(messages)
    }

    #[tracing::instrument(skip_all, fields(label_id=local_label_id.as_u64(), unread=?unread) )]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        session: &Session,
        stash: Stash,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        last_element_id: MessageId,
        last_element_time: UnixTimestamp,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Result<Vec<Message>, MailContextError> {
        tracing::info!(
            "Syncing next page in {remote_label_id:?} with end_id={last_element_id:?} and end={last_element_time}"
        );
        let mut response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(scroll_order == LabelScrollOrder::Descending),
                // time == 0 breaks the api query.
                end: Some(last_element_time.as_u64()),
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

        debug!(
            "Fetched {}/{} elements",
            response.messages.len(),
            response.total
        );

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];
        let mut tether = stash.connection();

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(
            local_label_id,
            &mut messages,
            unread,
            true,
            scroll_order,
            &mut tether,
        )
        .await?;

        Ok(messages)
    }

    #[tracing::instrument(skip_all, fields(label_id=local_label_id.as_u64(), unread=?unread) )]
    #[allow(clippy::too_many_arguments)]
    async fn sync_previous_page(
        session: &Session,
        stash: Stash,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        first_element_id: MessageId,
        first_element_time: UnixTimestamp,
        unread: ReadFilter,
        page_size: usize,
        scroll_order: LabelScrollOrder,
    ) -> Result<Vec<Message>, MailContextError> {
        tracing::info!(
            "Syncing previous page in {remote_label_id:?} with begin_id={first_element_id:?} and begin={first_element_time}"
        );
        let response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(scroll_order == LabelScrollOrder::Descending),
                // time == 0 breaks the api query.
                begin: Some(first_element_time.as_u64()),
                begin_id: Some(first_element_id.clone()),
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {}/{} elements",
            response.messages.len(),
            response.total
        );

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];
        let mut tether = stash.connection();
        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(
            local_label_id,
            &mut messages,
            unread,
            false,
            scroll_order,
            &mut tether,
        )
        .await?;

        Ok(messages)
    }

    async fn save_messages(
        local_label_id: LocalLabelId,
        messages: &mut [Message],
        unread: ReadFilter,
        update_scroller: bool,
        scroll_order: LabelScrollOrder,
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        if messages.is_empty() {
            return Ok(());
        }
        // We do not want to notify the UI about the not visible items
        // downloaded in the background
        tether
            .quiet_tx(async |tx| {
                // Save all messages.
                for message in messages.iter_mut() {
                    message.create_or_get_local(tx).await?
                }

                let last = messages.last().unwrap();
                let time = last.time;
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
                        display_order,
                        scroll_order,
                        tx,
                    )
                    .await?;
                }

                debug!(
                    "New last element id={:?}, time={}, order={}",
                    remote_id, time, display_order
                );

                Ok(())
            })
            .await
    }

    async fn update_scroller_data(
        local_label_id: LocalLabelId,
        remote_msg_id: MessageId,
        unread: ReadFilter,
        time: UnixTimestamp,
        display_order: u64,
        scroll_order: LabelScrollOrder,
        bond: &Bond<'_>,
    ) -> Result<MessageScrollData, MailContextError> {
        let mut msg_paginator = MessageScrollData::builder()
            .local_label_id(local_label_id)
            .unread(unread)
            .remote_message_id(remote_msg_id)
            .message_time(time)
            .display_order(display_order)
            .scroll_order(scroll_order)
            .build();

        msg_paginator.save(bond).await?;

        Ok(msg_paginator)
    }
}
