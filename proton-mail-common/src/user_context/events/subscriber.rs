use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::user_context::events::MailEvent;
use crate::MailUserContext;
use async_trait::async_trait;
use proton_api_mail::proton_api_core::exports::anyhow::anyhow;
use proton_api_mail::proton_api_core::exports::tracing::{debug, error};
use proton_event_loop::{Subscriber, SubscriberError};
use stash::orm::Model;
use std::sync::Weak;

pub struct MailEventSubscriber(Weak<MailUserContext>);

impl MailEventSubscriber {
    pub fn new(ctx: Weak<MailUserContext>) -> Self {
        Self(ctx)
    }
}

#[async_trait]
impl Subscriber<MailEvent> for MailEventSubscriber {
    fn name(&self) -> &str {
        "proton-mail-event-subscriber"
    }
    async fn on_events(&self, events: &mut [MailEvent]) -> Result<(), SubscriberError> {
        let ctx = self.0.upgrade().ok_or_else(|| {
            let e = anyhow!("MailUserContext no longer alive");
            error!("{e}");
            SubscriberError::Other(e)
        })?;

        let tx = ctx.user_context.stash().transaction().await?;

        {
            for event in events {
                let event = &event.event;
                if let Some(labels) = &event.labels {
                    debug!("Handling label events");
                    handle_label_events(&tx, labels)?;
                }

                if let Some(conversations) = &event.conversations {
                    debug!("Handling conversation events");
                    handle_conversation_events(&tx, conversations)?;
                }

                if let Some(messages) = &event.messages {
                    debug!("Handling message events");
                    handle_message_events(&tx, messages)?;
                }

                if let Some(conversation_counts) = &event.conversation_counts {
                    debug!("Handling conversation counts");
                    tx.create_or_update_conversation_counts(conversation_counts.iter())?;
                }

                if let Some(message_counts) = &event.message_counts {
                    debug!("Handling message counts");
                    tx.create_or_update_message_counts(message_counts.iter())?;
                }

                if let Some(mut mail_settings) = &event.mail_settings {
                    debug!("Handling mail settings");
                    mail_settings.save().await?;
                }
            }
            tx.commit().await
        }
        .map_err(|e| {
            let e = anyhow!("Failed to apply changes: {e}");
            error!("{e}");
            SubscriberError::Other(e)
        })
    }
}
