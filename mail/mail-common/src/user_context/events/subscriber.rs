use crate::actions::{conversations, messages};
use crate::datatypes::{MessageLabelsCount, ReadFilter, ViewMode};
use crate::models::default_location::IncomingDefaultLocation;
use crate::models::{
    ConversationScrollData, MailLabel, MailSettings, MessageScrollData, RollbackItem, ScrollCursor,
    StoreLabelCounters,
};
use crate::prefetch::PrefetchJob;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::{MailContextError, MailUserContext};
use crate::{datatypes::ConversationLabelsCount, events::MailEvent};
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use either::Either;
use proton_core_common::datatypes::{InitializedComponentState, SystemLabel};
use proton_core_common::models::{
    Address, Contact, InitializedComponent, Label, ModelExtension, User,
};
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use proton_task_service::AsyncTaskResult;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tracing::{debug, error};

pub struct MailEventSubscriber(Weak<MailUserContext>);

impl MailEventSubscriber {
    pub fn inner(&self) -> Result<Arc<MailUserContext>, anyhow::Error> {
        match self.0.upgrade() {
            Some(ctx) => Ok(ctx),
            None => bail!("MailUserContext no longer alive"),
        }
    }
}

impl MailEventSubscriber {
    pub fn new(ctx: Weak<MailUserContext>) -> Self {
        Self(ctx)
    }
}

#[async_trait]
impl Subscriber<MailEvent> for MailEventSubscriber {
    fn name(&self) -> &'static str {
        "proton-mail-event-subscriber"
    }

    async fn on_events(&self, events: &mut [MailEvent]) -> Result<(), SubscriberError> {
        let ctx = self.inner()?;
        debug!("Handling {} mail events", events.len());

        // This needs to happen outside of the transaction because queuing an action creates a
        // transaction and it would deadlock otherwise.
        let mut tether = ctx.user_context.stash().connection();
        let mut conversation_ids = Vec::with_capacity(events.len());
        let mut message_ids = Vec::with_capacity(events.len());
        let queue_incoming_default = tether
            .tx::<_, _, SubscriberError>(async |tx| {
                let mut queue_incoming_default = false;
                for event in events {
                    if let Some(labels) = &event.labels {
                        debug!("Handling label events");
                        handle_label_events(tx, labels).await?;
                    }

                    if let Some(conversations) = &event.conversations {
                        debug!("Handling conversation events");
                        let ids = handle_conversation_events(tx, conversations)
                            .await
                            .map_err(|e| {
                                error!("{e:?}");
                                SubscriberError::Other(e.into())
                            })?;
                        conversation_ids.extend(ids);
                    }

                    if let Some(messages) = &event.messages {
                        debug!("Handling message events");
                        let ids = handle_message_events(tx, messages).await.map_err(|e| {
                            error!("{e:?}");
                            SubscriberError::Other(e.into())
                        })?;

                        message_ids.extend(ids);
                    }

                    if let Some(conversation_counts) = &event.conversation_counts {
                        debug!("Handling conversation counts");
                        ConversationLabelsCount::create_or_update_conversation_counts(
                            conversation_counts.clone(),
                            tx,
                        )
                        .await?;
                    }

                    if let Some(message_counts) = &event.message_counts {
                        debug!("Handling message counts");
                        MessageLabelsCount::create_or_update_message_counts(
                            message_counts.clone(),
                            tx,
                        )
                        .await?;
                    }

                    if let Some(mail_settings) = event.mail_settings.as_mut() {
                        debug!("Handling mail settings");
                        mail_settings.save(tx).await?;
                    }

                    // It so happens that the API only returns the IDs of what changed, not the
                    // actual data, so we better reload all.
                    queue_incoming_default |= event.incoming_defaults.is_some();
                }
                Ok(queue_incoming_default)
            })
            .await
            .map_err(|e| {
                let e = anyhow!("Failed to apply changes: {e}");
                error!("{e:?}");
                SubscriberError::Other(e)
            })?;

        let label_id = SystemLabel::AllMail.local_id(&tether).await?.unwrap();
        let conversation_jobs = conversation_ids
            .into_iter()
            .map(|id| PrefetchJob::Conversation(id, label_id))
            .collect();
        let message_jobs = message_ids.into_iter().map(PrefetchJob::Message).collect();

        let _ = ctx
            .queue_prefetch_jobs(conversation_jobs)
            .await
            .inspect_err(|e| {
                error!("Failed to queue cnv jobs for prefetch: {e}");
            });
        let _ = ctx
            .queue_prefetch_jobs(message_jobs)
            .await
            .inspect_err(|e| {
                error!("Failed to queue msg jobs for prefetch: {e}");
            });

        if queue_incoming_default {
            IncomingDefaultLocation::action_resync(ctx.action_queue()).await;
        }

        Ok(())
    }

    async fn on_refresh(&self, event: &MailEvent) -> Result<(), SubscriberError> {
        debug!("Handling refresh event");
        let ctx = self.inner()?;

        match event.refresh {
            0 => return Ok(()),
            1 => refresh_mail(ctx.clone()).await?,
            2 => refresh_core(ctx.clone()).await?,
            255 => {
                refresh_core(ctx.clone()).await?;
                refresh_mail(ctx.clone()).await?;
            }
            e => {
                error!("Unhandled refresh event: `{e}`");
                return Err(SubscriberError::Other(anyhow!(
                    "Unhandled refresh event: `{e}`"
                )));
            }
        }

        let mut tether = ctx.user_context.stash().connection();
        InitializedComponent::set_state(
            MailUserContext::CONTEXT_INIT_KEY,
            InitializedComponentState::NotInitialized,
            &mut tether,
        )
        .await?;

        // And run the initialization again
        MailUserContext::initialize_async(ctx.as_arc())
            .await
            .map_err(|e| {
                let e = anyhow!("Failed to re-initialize MailUserContext: {e}");
                error!("{e:?}");
                SubscriberError::Other(e)
            })?;

        Ok(())
    }
}

async fn refresh_mail(ctx: Arc<MailUserContext>) -> Result<(), SubscriberError> {
    debug!("Handling refresh event");
    let api = ctx.api().clone();
    let all_remote_labels = ctx.spawn(async move { Label::all_labels(&api).await });
    let mut tether = ctx.user_context.stash().connection();
    let mut all_local_labels: HashMap<_, _> = Label::all(&tether)
        .await?
        .into_iter()
        .map(|label| (label.remote_id.clone(), label))
        .collect();
    let all_mail = SystemLabel::AllMail
        .load(&tether)
        .await?
        .ok_or_else(|| anyhow!("All mail label is missing!"))?;
    let scroll_cursor = match all_mail.view_mode(&tether).await? {
        ViewMode::Conversations => {
            Either::Left(ScrollCursor::<ConversationScrollData>::absolute_end(
                all_mail.local_id.unwrap(),
                ReadFilter::All,
            ))
        }
        ViewMode::Messages => Either::Right(ScrollCursor::<MessageScrollData>::absolute_end(
            all_mail.local_id.unwrap(),
            ReadFilter::All,
        )),
    };
    let AsyncTaskResult::Completed(Ok(all_remote_labels)) = all_remote_labels
        .await
        .map_err(|e| anyhow!("Failed to download remote labels: `{e}`"))?
    else {
        return Err(SubscriberError::Other(anyhow!(
            "The task was cancelled, we need to run refresh again"
        )));
    };

    for remote_label in all_remote_labels.iter() {
        all_local_labels.remove(&remote_label.remote_id);
    }

    tether
        .tx::<_, _, MailContextError>(async |tx| {
            RollbackItem::delete_all(tx).await?;
            ConversationScrollData::delete_all(tx).await?;
            MessageScrollData::delete_all(tx).await?;

            Label::sync_labels(tx, all_remote_labels).await?;

            for removed_local_label in all_local_labels.into_values() {
                removed_local_label.delete(tx).await?;
            }

            Ok(())
        })
        .await
        .map_err(|e| {
            let e = anyhow!("Failed to clear database entries: {e}");
            error!("{e:?}");
            SubscriberError::Other(e)
        })?;

    match scroll_cursor {
        // TODO: Maybe limit prefetch jobs to some reasonable number
        Either::Left(conv_scroll_cursor) => {
            let actions = conv_scroll_cursor
                .visible_elements(&tether)
                .await?
                .into_iter()
                .map(|conv| conversations::RefreshMetadata::new(conv.local_id));

            for action in actions {
                if let Err(error) = ctx.action_queue().queue_action(action).await {
                    error!("Failed to refresh conversation metadata: `{error}`",);
                }
            }
        }
        Either::Right(msg_scroll_cursor) => {
            let actions = msg_scroll_cursor
                .visible_elements(&tether)
                .await?
                .into_iter()
                .filter_map(|msg| msg.local_id)
                .map(messages::RefreshMetadata::new);

            for action in actions {
                if let Err(error) = ctx.action_queue().queue_action(action).await {
                    error!("Failed to refresh message metadata: `{error}`",);
                }
            }
        }
    };

    InitializedComponent::set_state(
        StoreLabelCounters::INIT_KEY,
        InitializedComponentState::NotInitialized,
        &mut tether,
    )
    .await?;
    InitializedComponent::set_state(
        MailSettings::INIT_KEY,
        InitializedComponentState::NotInitialized,
        &mut tether,
    )
    .await?;
    InitializedComponent::set_state(
        IncomingDefaultLocation::INIT_KEY,
        InitializedComponentState::NotInitialized,
        &mut tether,
    )
    .await?;

    Ok(())
}

async fn refresh_core(ctx: Arc<MailUserContext>) -> Result<(), SubscriberError> {
    let api = ctx.api().clone();
    let all_remote_addresses = ctx.spawn(async move { Address::sync(&api).await });
    let mut tether = ctx.user_context.stash().connection();
    let mut all_local_addresses: HashMap<_, _> = Address::all(&tether)
        .await?
        .into_iter()
        .map(|addr| (addr.remote_id.clone(), addr))
        .collect();
    let AsyncTaskResult::Completed(Ok(all_remote_addresses)) = all_remote_addresses
        .await
        .map_err(|e| anyhow!("Failed to download remote addresses: `{e}`"))?
    else {
        return Err(SubscriberError::Other(anyhow!(
            "The task was cancelled, we need to run refresh again"
        )));
    };
    let all_remote_addresses = all_remote_addresses.inner();

    for remote_label in all_remote_addresses.iter() {
        all_local_addresses.remove(&remote_label.remote_id);
    }

    tether
        .tx::<_, _, SubscriberError>(async |tx| {
            Contact::delete_all(tx).await?;

            for removed_local_address in all_local_addresses.into_values() {
                removed_local_address.delete(tx).await?;
            }
            for mut remote_address in all_remote_addresses {
                remote_address.save(tx).await?;
            }

            Ok(())
        })
        .await
        .map_err(|e| {
            let e = anyhow!("Failed to clear database entries: {e}");
            error!("{e:?}");
            SubscriberError::Other(e)
        })?;

    // Core
    //
    // User and UserSettings have common initialization path
    InitializedComponent::set_state(
        User::INIT_KEY,
        InitializedComponentState::NotInitialized,
        &mut tether,
    )
    .await?;
    InitializedComponent::set_state(
        Contact::INIT_KEY,
        InitializedComponentState::NotInitialized,
        &mut tether,
    )
    .await?;

    Ok(())
}

pub async fn whole_refresh_tui(ctx: Arc<MailUserContext>) -> Result<(), SubscriberError> {
    refresh_core(ctx.clone()).await?;
    refresh_mail(ctx.clone()).await?;

    let mut tether = ctx.user_context.stash().connection();
    InitializedComponent::set_state(
        MailUserContext::CONTEXT_INIT_KEY,
        InitializedComponentState::NotInitialized,
        &mut tether,
    )
    .await?;

    // And run the initialization again
    MailUserContext::initialize_async(ctx.as_arc())
        .await
        .map_err(|e| {
            let e = anyhow!("Failed to re-initialize MailUserContext: {e}");
            error!("{e:?}");
            SubscriberError::Other(e)
        })?;

    Ok(())
}
