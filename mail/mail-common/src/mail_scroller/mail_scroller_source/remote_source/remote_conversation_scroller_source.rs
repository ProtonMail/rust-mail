use super::{MailPaginatorJoinHandle, RemoteSource};
use crate::datatypes::dependencies::MessageOrConversationDependencyFetcher;
use crate::datatypes::labels::ScrollOrderDir;
use crate::datatypes::labels::ScrollOrderField;
#[cfg(feature = "prefetch")]
use crate::prefetch::PrefetchJob;
use crate::{
    MailContextError, MailUserContext,
    datatypes::{ContextualConversation, ReadFilter},
    models::{Conversation, ConversationScrollData},
};
use anyhow::anyhow;
use proton_core_api::{services::proton::LabelId, session::Session};
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_mail_api::services::proton::{
    ProtonMail,
    common::ConversationId,
    prelude::{GetConversationsOptions, GetConversationsResponse},
};
use stash::stash::{Bond, Stash, Tether};
use tracing::debug;

/// Mail scroller implementation for [`Conversation`] on in a [`Label`].
///
/// The scroller keeps track of the last element returned by the server for the
/// selected label and read filter. This element is then used to fetch
/// new data from the server.
#[derive(Debug)]
pub(super) struct RemoteConversationScrollerSource;

impl RemoteSource for ConversationScrollData {
    async fn sync_first_page(
        ctx: &MailUserContext,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        invalidate: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let session = ctx.session().clone();
        let stash = ctx.user_stash().clone();

        #[cfg(feature = "prefetch")]
        let arc_ctx = ctx.as_arc();

        let handle = ctx.spawn(async move {
            #[allow(unused_variables)]
            let items = RemoteConversationScrollerSource::sync_first_page(
                &session,
                stash,
                local_label_id,
                remote_label_id,
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

            #[cfg(feature = "prefetch")]
            {
                let prefetch_jobs = items
                    .into_iter()
                    .filter(|item| !item.has_messages)
                    .map(|item| PrefetchJob::Conversation(item.local_id, local_label_id))
                    .collect();

                arc_ctx.queue_prefetch_jobs(prefetch_jobs).await?;
            }

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
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        RemoteConversationScrollerSource::spawn_background_sync(
            ctx,
            scroller,
            local_label_id,
            remote_label_id,
            unread,
            page_size,
            order_dir,
            order_field,
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
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        sender: Option<flume::Sender<()>>,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let remote_id = scroller.remote_conversation_id.clone();
        let context_time = scroller.context_time(order_field);
        let session = ctx.session().clone();

        let task = Some(ctx.spawn(async move {
            let items = RemoteConversationScrollerSource::sync_previous_page(
                &session,
                stash,
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
        }));

        Ok(task)
    }
}

impl RemoteConversationScrollerSource {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn spawn_background_sync(
        ctx: &MailUserContext,
        scroller: &ConversationScrollData,
        label_local_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<MailPaginatorJoinHandle, MailContextError> {
        let stash = ctx.user_stash().clone();
        let remote_id = scroller.remote_conversation_id.clone();
        let context_time = scroller.context_time(order_field);
        let session = ctx.session().clone();

        let task = Some(ctx.spawn(async move {
            Self::sync_next_page(
                &session,
                stash,
                label_local_id,
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
        }));

        Ok(task)
    }

    #[tracing::instrument(skip_all, fields(label_id=local_label_id.as_u64(), unread=?unread) )]
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn sync_first_page(
        session: &Session,
        stash: Stash,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<Vec<ContextualConversation>, MailContextError> {
        tracing::info!("Syncing first page in {remote_label_id:?}");

        let response = session
            .get_conversations(GetConversationsOptions {
                label_id: Some(remote_label_id),
                page_size: page_size as u64,
                unread: unread.into(),
                desc: order_dir.as_api_desc(),
                sort: order_field.as_api_sort(),
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {}/{} elements",
            response.conversations.len(),
            response.total
        );

        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        let context_time = Self::context_time(&response, unread);

        let mut conversations: Vec<Conversation> = response
            .conversations
            .into_iter()
            .map(|c| c.into())
            .collect();

        let mut tether = stash.connection().await?;

        Self::save_conversations(
            local_label_id,
            &mut conversations,
            unread,
            context_time,
            true,
            order_dir,
            order_field,
            session,
            &mut tether,
        )
        .await?;

        Ok(Self::contextual_conversations(
            local_label_id,
            conversations,
        ))
    }

    #[tracing::instrument(skip_all, fields(label_id=local_label_id.as_u64(), unread=?unread) )]
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn sync_previous_page(
        session: &Session,
        stash: Stash,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        first_element_id: ConversationId,
        first_element_time: UnixTimestamp,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<Vec<ContextualConversation>, MailContextError> {
        tracing::info!(
            "Syncing previous page in {remote_label_id:?} with begin_id={first_element_id:?} and begin={first_element_time}"
        );

        let response = session
            .get_conversations(GetConversationsOptions {
                anchor: Some(first_element_time.as_u64()),
                anchor_id: Some(first_element_id.clone()),
                label_id: Some(remote_label_id),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                desc: order_dir.reverse().as_api_desc(),
                sort: order_field.as_api_sort(),
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {}/{} elements",
            response.conversations.len(),
            response.total
        );

        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        let context_time = Self::context_time(&response, unread);

        let mut conversations: Vec<Conversation> = response
            .conversations
            .into_iter()
            .map(|c| c.into())
            .collect();

        let mut tether = stash.connection().await?;

        Self::save_conversations(
            local_label_id,
            &mut conversations,
            unread,
            context_time,
            false,
            order_dir,
            order_field,
            session,
            &mut tether,
        )
        .await?;

        Ok(Self::contextual_conversations(
            local_label_id,
            conversations,
        ))
    }

    #[tracing::instrument(skip_all, fields(label_id=local_label_id.as_u64(), unread=?unread) )]
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn sync_next_page(
        session: &Session,
        stash: Stash,
        local_label_id: LocalLabelId,
        remote_label_id: LabelId,
        last_element_id: ConversationId,
        last_element_time: UnixTimestamp,
        unread: ReadFilter,
        page_size: usize,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
    ) -> Result<Vec<ContextualConversation>, MailContextError> {
        tracing::info!(
            "Syncing next page in {remote_label_id:?} with end_id={last_element_id:?} and end={last_element_time}"
        );

        let mut response = session
            .get_conversations(GetConversationsOptions {
                // time == 0 breaks the api query.
                anchor: Some(last_element_time.as_u64()),
                anchor_id: Some(last_element_id.clone()),
                label_id: Some(remote_label_id),
                page_size: page_size as u64 + 1_u64,
                unread: unread.into(),
                desc: order_dir.as_api_desc(),
                sort: order_field.as_api_sort(),
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

        debug!(
            "Fetched {}/{} elements",
            response.conversations.len(),
            response.total
        );

        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        let context_time = Self::context_time(&response, unread);

        let mut conversations: Vec<Conversation> = response
            .conversations
            .into_iter()
            .map(|c| c.into())
            .collect();

        let mut tether = stash.connection().await?;

        Self::save_conversations(
            local_label_id,
            &mut conversations,
            unread,
            context_time,
            true,
            order_dir,
            order_field,
            session,
            &mut tether,
        )
        .await?;

        Ok(Self::contextual_conversations(
            local_label_id,
            conversations,
        ))
    }

    fn context_time(
        response: &GetConversationsResponse,
        unread: ReadFilter,
    ) -> Option<UnixTimestamp> {
        if unread != ReadFilter::All {
            // When filtering conversations, we need to use the contextual time
            // perform the next page query or the data will not be displayed
            // correctly.
            // This contextual time also does not match the ConversationLabel.context_time
            // we use to display the query results. This means that the data
            // will change after it is written to the database.
            response.conversations.last()?.context_time.map(Into::into)
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

    #[allow(clippy::too_many_arguments)]
    async fn save_conversations(
        local_label_id: LocalLabelId,
        conversations: &mut [Conversation],
        unread: ReadFilter,
        context_time: Option<UnixTimestamp>,
        update_scroller: bool,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        api: &Session,
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        // Resolve missing dependencies.
        let mut dependency_fetcher = MessageOrConversationDependencyFetcher::new();
        for conversation in conversations.iter() {
            dependency_fetcher
                .check_conversation(conversation, tether)
                .await?;
        }
        dependency_fetcher.fetch_and_store(api, tether).await?;

        // We do not want to notify the UI about the not visible items
        // downloaded in the background
        tether
            .quiet_tx(async |tx| {
                // Save all conversations.
                for conversation in conversations.iter_mut() {
                    conversation.create_or_get_local(tx).await?;
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

                let snooze_time = label.context_snooze_time;
                let context_time = context_time.unwrap_or(label.context_time);

                // Unwrap safety: RemoteId is present as this method is called on conversation
                // downloaded from API
                let remote_id = last.remote_id.clone().unwrap();
                let display_order = last.display_order;

                if update_scroller {
                    Self::update_scroller_data(
                        local_label_id,
                        remote_id.clone(),
                        unread,
                        context_time,
                        snooze_time,
                        display_order,
                        order_dir,
                        order_field,
                        tx,
                    )
                    .await?;
                }

                Ok(())
            })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_scroller_data(
        local_label_id: LocalLabelId,
        remote_conv_id: ConversationId,
        unread: ReadFilter,
        context_time: UnixTimestamp,
        snooze_time: UnixTimestamp,
        display_order: u64,
        order_dir: ScrollOrderDir,
        order_field: ScrollOrderField,
        bond: &Bond<'_>,
    ) -> Result<ConversationScrollData, MailContextError> {
        tracing::debug!(
            "New conversation cursor {remote_conv_id} at time={context_time}, snooze_time={snooze_time}, order={display_order}"
        );
        let mut conv_paginator = ConversationScrollData::builder()
            .local_label_id(local_label_id)
            .unread(unread)
            .remote_conversation_id(remote_conv_id)
            .conversation_time(context_time)
            .snooze_time(snooze_time)
            .display_order(display_order)
            .order_dir(order_dir)
            .order_field(order_field)
            .build();

        conv_paginator.save(bond).await?;

        Ok(conv_paginator)
    }
}
