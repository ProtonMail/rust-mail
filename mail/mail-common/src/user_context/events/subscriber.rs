use crate::MailUserContext;
use crate::datatypes::MessageLabelsCount;
use crate::models::default_location::IncomingDefaultLocation;
use crate::prefetch::PrefetchJob;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::{datatypes::ConversationLabelsCount, events::MailEvent};
use anyhow::anyhow;
use async_trait::async_trait;
use proton_core_common::datatypes::SystemLabel;
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use std::sync::Weak;
use tracing::{debug, error};

pub struct MailEventSubscriber(Weak<MailUserContext>);

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
        let ctx = self.0.upgrade().ok_or_else(|| {
            let e = anyhow!("MailUserContext no longer alive");
            error!("{e:?}");
            SubscriberError::Other(e)
        })?;
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
}
