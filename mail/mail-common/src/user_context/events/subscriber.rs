use crate::MailUserContext;
use crate::datatypes::MessageLabelsCount;
use crate::models::default_location::IncomingDefaultLocation;
use crate::models::{Conversation, MailSettings, Message, StoreLabelCounters};
use crate::prefetch::PrefetchJob;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::{datatypes::ConversationLabelsCount, events::MailEvent};
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use proton_core_common::datatypes::{InitializedComponentState, SystemLabel};
use proton_core_common::models::{
    Address, Contact, InitializedComponent, Label, ModelExtension, User, UserSettings,
};
use proton_core_common::nuke_utils::clear_dir_safe;
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
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

    async fn on_refresh(&self, _event: &MailEvent) -> Result<(), SubscriberError> {
        let ctx = self.inner()?;
        debug!("Handling refresh event");
        let mut tether = ctx.user_context.stash().connection();
        tether
            .tx::<_, _, SubscriberError>(async |tx| {
                // Mail
                Label::delete_all(tx).await?;
                Conversation::delete_all(tx).await?;
                Message::delete_all(tx).await?;
                MailSettings::delete_all(tx).await?;
                // Core
                User::delete_all(tx).await?;
                UserSettings::delete_all(tx).await?;
                Contact::delete_all(tx).await?;
                Address::delete_all(tx).await?;

                Ok(())
            })
            .await
            .map_err(|e| {
                let e = anyhow!("Failed to clear database entries: {e}");
                error!("{e:?}");
                SubscriberError::Other(e)
            })?;

        let user_id = ctx.user_id();
        let mail_cache = ctx.mail_context().mail_cache_path(user_id);
        // Clear all cached data as it no longer is
        // assigned to the correct items in database.
        // The folder structure will remain in place.
        clear_dir_safe(mail_cache).await;

        // Reset initialization state so we are able
        // to run an initialization again
        //
        // Mail
        InitializedComponent::set_state(
            Label::INIT_KEY,
            InitializedComponentState::NotInitialized,
            &mut tether,
        )
        .await?;
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
        InitializedComponent::set_state(
            Address::INIT_KEY,
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
