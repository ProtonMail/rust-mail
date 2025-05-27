use crate::MailUserContext;
use crate::actions::refresh::ActionRefresh;
use crate::actions::{conversations, messages};
use crate::datatypes::{MessageLabelsCount, ReadFilter, ViewMode};
use crate::models::default_location::IncomingDefaultLocation;
use crate::models::{
    CachedScrollData, ConversationScrollData, MailLabel, MailSettings, MessageScrollData,
    RollbackItem, StoreLabelCounters,
};
use crate::prefetch::PrefetchJob;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::{datatypes::ConversationLabelsCount, events::MailEvent};
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use either::Either;
use proton_action_queue::queue::{ActionError as QueueActionError, QueuedActionOutput};
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{Address, Contact, Label, ModelExtension, User};
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use proton_task_service::AsyncTaskResult;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tracing::{debug, error, info, warn};

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
        let ctx = self.inner()?;

        ctx.on_refresh_impl(event.refresh).await
    }
}

impl MailUserContext {
    pub async fn refresh_action(
        &self,
    ) -> Result<QueuedActionOutput<ActionRefresh>, QueueActionError<ActionRefresh>> {
        self.action_queue().queue_action(ActionRefresh {}).await
    }

    pub(crate) async fn on_refresh_impl(
        self: Arc<Self>,
        refresh: u8,
    ) -> Result<(), SubscriberError> {
        debug!("Handling refresh event");
        let ctx = self;

        macro_rules! try_refresh {
            ($fn_name:tt) => {{
                let max_attempts = 2;
                let mut attempts = 0;

                while let Err(e) = $fn_name(ctx.clone()).await {
                    if attempts >= max_attempts {
                        return Err(e);
                    }
                    attempts += 1;
                }
            }};
        }

        match refresh {
            0 => {
                warn!("Nothing to refresh, this may idicate bug in SDK event loop implementation");
                return Ok(());
            }
            1 => {
                info!("Handling mail refresh");
                try_refresh!(refresh_mail);
            }
            2 => {
                info!("Handling contacts refresh");
                try_refresh!(refresh_contacts);
            }
            255 => {
                info!("Handling refresh all");
                try_refresh!(refresh_core);
                try_refresh!(refresh_mail);
            }
            e => {
                error!("Unhandled refresh event: `{e}`");
                return Err(SubscriberError::Other(anyhow!(
                    "Unhandled refresh event: `{e}`"
                )));
            }
        }

        Ok(())
    }
}

macro_rules! join_task {
    ($name:tt, $description: expr) => {{
        if let AsyncTaskResult::Completed(Ok(value)) = $name
            .await
            .map_err(|e| anyhow!("Failed to download remote {}: `{e}`", $description))?
        {
            value
        } else {
            return Err(SubscriberError::Other(anyhow!(
                "The task was cancelled, we need to run refresh again"
            )));
        }
    }};
}

#[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
async fn refresh_mail(ctx: Arc<MailUserContext>) -> Result<(), SubscriberError> {
    let api = ctx.api().clone();
    let all_remote_labels = ctx.spawn(async move { Label::fetch_mail_labels(&api).await });
    let api = ctx.api().clone();
    let counters = ctx.spawn(async move { StoreLabelCounters::fetch(&api).await });
    let api = ctx.api().clone();
    let mail_settings = ctx.spawn(async move { MailSettings::sync_mail_settings(&api).await });

    let mut tether = ctx.user_context.stash().connection();
    let mut all_local_labels: HashMap<_, _> = Label::all_mail(&tether)
        .await?
        .into_iter()
        .map(|label| (label.remote_id.clone(), label))
        .collect();
    debug!(
        "Number of labels available localy: {}",
        all_local_labels.len()
    );
    let all_mail = SystemLabel::AllMail
        .load(&tether)
        .await?
        .ok_or_else(|| anyhow!("All mail label is missing!"))?;
    let page_size = 25;
    let scroll_cursor = match all_mail.view_mode(&tether).await? {
        ViewMode::Conversations => Either::Left(CachedScrollData::<ConversationScrollData>::all(
            all_mail.local_id.unwrap(),
            ReadFilter::All,
            page_size,
        )),
        ViewMode::Messages => Either::Right(CachedScrollData::<MessageScrollData>::all(
            all_mail.local_id.unwrap(),
            ReadFilter::All,
            page_size,
        )),
    };

    let all_remote_labels = join_task!(all_remote_labels, "labels");
    let counters = join_task!(counters, "label counters");
    let mail_settings = join_task!(mail_settings, "mail settings");

    debug!(
        "Number of labels available remotely: {}",
        all_remote_labels.len()
    );
    for remote_label in all_remote_labels.iter() {
        all_local_labels.remove(&remote_label.remote_id);
    }

    tether
        .tx::<_, _, SubscriberError>(async |tx| {
            RollbackItem::delete_all(tx).await?;
            ConversationScrollData::delete_all(tx).await?;
            MessageScrollData::delete_all(tx).await?;

            Label::sync_labels(tx, all_remote_labels)
                .await
                .map_err(|e| {
                    let e = anyhow!("Failed to sync labels: {e}");
                    error!("{e:?}");
                    SubscriberError::Other(e)
                })?;

            for local_label_to_remove in all_local_labels.into_values() {
                if let Some(_system_label) =
                    SystemLabel::from_rid(local_label_to_remove.remote_id.as_ref())
                {
                    // For some reason API does not return all system labels
                    // we have to make sure to not delete those
                    continue;
                }

                debug!(
                    "Removing label with remote_id {:?}",
                    local_label_to_remove.remote_id
                );
                local_label_to_remove.delete(tx).await?;
            }
            counters.save(tx).await?;
            mail_settings.store(tx).await?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to clear database entries, while refreshing mail: {e}");
        })?;

    IncomingDefaultLocation::action_resync(ctx.action_queue()).await;

    match scroll_cursor {
        Either::Left(mut conv_scroll_cursor) => {
            debug!(
                "Queue conversations to refresh, count: {}",
                conv_scroll_cursor.all_element_count(&tether).await?
            );

            loop {
                let page = conv_scroll_cursor.fetch_more(&tether).await?;
                if page.is_empty() {
                    break;
                }
                let local_conv_ids = page.into_iter().map(|conv| conv.local_id).collect();

                let action = conversations::RefreshMetadata::new(local_conv_ids);
                if let Err(error) = ctx.action_queue().queue_action(action).await {
                    error!("Failed to refresh conversation metadata: `{error}`",);
                }
            }
        }
        Either::Right(mut msg_scroll_cursor) => {
            debug!(
                "Queue messages to refresh, count: {}",
                msg_scroll_cursor.all_element_count(&tether).await?
            );

            loop {
                let page = msg_scroll_cursor.fetch_more(&tether).await?;
                if page.is_empty() {
                    break;
                }
                let local_msg_ids = page.into_iter().filter_map(|msg| msg.local_id).collect();
                let action = messages::RefreshMetadata::new(local_msg_ids);

                if let Err(error) = ctx.action_queue().queue_action(action).await {
                    error!("Failed to refresh message metadata: `{error}`",);
                }
            }
        }
    };

    Ok(())
}

#[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
async fn refresh_core(ctx: Arc<MailUserContext>) -> Result<(), SubscriberError> {
    let api = ctx.api().clone();
    let contacts = ctx.spawn(async move { Contact::sync(&api).await });
    let api = ctx.api().clone();
    let all_remote_addresses = ctx.spawn(async move { Address::sync(&api).await });
    let api = ctx.api().clone();
    let user_and_settings = ctx.spawn(async move { User::sync_user_and_settings(&api).await });
    let api = ctx.api().clone();
    let all_remote_labels = ctx.spawn(async move { Label::fetch_contact_labels(&api).await });

    let mut tether = ctx.user_context.stash().connection();
    let mut all_local_addresses: HashMap<_, _> = Address::all(&tether)
        .await?
        .into_iter()
        .map(|addr| (addr.remote_id.clone(), addr))
        .collect();
    let mut all_local_labels: HashMap<_, _> = Label::all_contacts(&tether)
        .await?
        .into_iter()
        .map(|label| (label.remote_id.clone(), label))
        .collect();
    debug!(
        "Number of labels available localy: {}",
        all_local_labels.len()
    );

    debug!(
        "Number of addresses available localy: {}",
        all_local_addresses.len()
    );

    let all_remote_addresses = join_task!(all_remote_addresses, "addresses").inner();
    let user_and_settings = join_task!(user_and_settings, "user and settings");
    let all_remote_labels = join_task!(all_remote_labels, "labels");

    debug!(
        "Number of addresses available remotely: {}",
        all_remote_addresses.len()
    );
    for remote_label in all_remote_addresses.iter() {
        all_local_addresses.remove(&remote_label.remote_id);
    }
    debug!(
        "Number of labels available remotely: {}",
        all_remote_labels.len()
    );
    for remote_label in all_remote_labels.iter() {
        all_local_labels.remove(&remote_label.remote_id);
    }

    let contacts = join_task!(contacts, "contacts");

    tether
        .tx::<_, _, SubscriberError>(async |tx| {
            for local_address_to_remove in all_local_addresses.into_values() {
                debug!(
                    "Removing address with remote_id {:?}",
                    local_address_to_remove.remote_id
                );
                local_address_to_remove.delete(tx).await?;
            }
            for mut remote_address in all_remote_addresses {
                remote_address.save(tx).await?;
            }

            Label::sync_labels(tx, all_remote_labels)
                .await
                .map_err(|e| {
                    let e = anyhow!("Failed to sync labels: {e}");
                    error!("{e:?}");
                    SubscriberError::Other(e)
                })?;

            for local_label_to_remove in all_local_labels.into_values() {
                debug!(
                    "Removing label with remote_id {:?}",
                    local_label_to_remove.remote_id
                );
                local_label_to_remove.delete(tx).await?;
            }
            user_and_settings.store(tx).await?;
            contacts.store(tx).await?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to clear database entries while refreshing core: {e}");
        })?;

    Ok(())
}

#[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
async fn refresh_contacts(ctx: Arc<MailUserContext>) -> Result<(), SubscriberError> {
    let contacts = Contact::sync(ctx.api()).await?;
    let mut tether = ctx.user_context.stash().connection();

    tether
        .tx::<_, _, SubscriberError>(async |tx| {
            contacts.store(tx).await?;

            Ok(())
        })
        .await
        .inspect_err(|e| {
            error!("Failed to clear database entries while refreshing core: {e}");
        })?;

    Ok(())
}
