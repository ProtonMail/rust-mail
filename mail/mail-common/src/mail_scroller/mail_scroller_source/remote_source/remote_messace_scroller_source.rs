use proton_api_core::{
    services::proton::LabelId,
    session::{CoreSession, Session},
};
use proton_api_mail::services::proton::{
    ProtonMail, common::MessageId, prelude::GetMessagesOptions,
};
use proton_core_common::datatypes::LocalLabelId;
use stash::stash::{Bond, Stash, Tether};
use tracing::debug;

use crate::{
    MailContextError, MailUserContext,
    datatypes::ReadFilter,
    models::{Message, MessageScrollData},
};

use super::{MailPaginatorJoinHandle, RemoteSource};

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
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let session = ctx.session().clone();
        let stash = ctx.user_stash().clone();
        let handle = ctx.spawn(async move {
            RemoteMessageScrollerSource::sync_first_page(
                &session,
                stash,
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

    async fn sync_next_page(
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

    async fn sync_previous_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        scroller: &Self,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let remote_id = scroller.remote_message_id.clone();
        let message_time = scroller.message_time;
        let session = ctx.session().clone();

        let task = Some(ctx.spawn(async move {
            RemoteMessageScrollerSource::sync_previous_page(
                &session,
                stash,
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
}

impl RemoteMessageScrollerSource {
    pub(super) async fn spawn_background_sync(
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
            )
            .await?;

            Ok(())
        }));

        Ok(task)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,stash,local_label_id, remote_label_id))]
    pub(super) async fn sync_first_page(
        session: &Session,
        stash: Stash,
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
        let mut tether = stash.connection();

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(local_label_id, &mut messages, unread, true, &mut tether).await?;

        Ok(messages)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,stash,local_label_id, remote_label_id))]
    #[allow(clippy::too_many_arguments)]
    async fn sync_next_page(
        session: &Session,
        stash: Stash,
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
        let mut tether = stash.connection();

        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(local_label_id, &mut messages, unread, true, &mut tether).await?;

        Ok(messages)
    }

    #[tracing::instrument(level = tracing::Level::DEBUG, skip(session,stash,local_label_id, remote_label_id))]
    #[allow(clippy::too_many_arguments)]
    async fn sync_previous_page(
        session: &Session,
        stash: Stash,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        first_element_id: MessageId,
        first_element_time: u64,
        unread: ReadFilter,
        page_size: usize,
    ) -> Result<Vec<Message>, MailContextError> {
        debug!("Syncing previous page");
        let response = session
            .api()
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                // time == 0 breaks the api query.
                begin: Some(first_element_time),
                begin_id: Some(first_element_id.clone()),
                label_id: Some(vec![remote_label_id]),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                ..Default::default()
            })
            .await?;

        debug!("Fetched {} elements", response.messages.len());

        if response.messages.is_empty() {
            return Ok(vec![]);
        }

        let mut messages: Vec<Message> = vec![];
        let mut tether = stash.connection();
        for message in response.messages {
            messages.push(Message::from_api_metadata(message, &tether).await?);
        }

        Self::save_messages(local_label_id, &mut messages, unread, false, &mut tether).await?;

        Ok(messages)
    }

    async fn save_messages(
        local_label_id: LocalLabelId,
        messages: &mut [Message],
        unread: ReadFilter,
        update_scroller: bool,
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
