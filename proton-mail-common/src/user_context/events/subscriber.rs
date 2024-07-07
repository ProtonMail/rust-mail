use crate::events::MailEvent;
use crate::models::Label;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::MailUserContext;
use anyhow::anyhow;
use async_trait::async_trait;
use proton_event_loop::subscriber::{Subscriber, SubscriberError};
use stash::orm::Model;
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
                if let Some(labels) = &event.labels {
                    debug!("Handling label events");
                    handle_label_events(&tx, labels).await?;
                }

                if let Some(conversations) = &event.conversations {
                    debug!("Handling conversation events");
                    handle_conversation_events(&tx, conversations)
                        .await
                        .map_err(|e| {
                            error!("{e}");
                            SubscriberError::Other(e.into())
                        })?;
                }

                if let Some(messages) = &event.messages {
                    debug!("Handling message events");
                    handle_message_events(&tx, messages).await.map_err(|e| {
                        error!("{e}");
                        SubscriberError::Other(e.into())
                    })?;
                }

                if let Some(conversation_counts) = &event.conversation_counts {
                    debug!("Handling conversation counts");
                    Label::create_or_update_conversation_counts(
                        conversation_counts.clone(),
                        tx.stash(),
                    )
                    .await?;
                }

                if let Some(message_counts) = &event.message_counts {
                    debug!("Handling message counts");
                    Label::create_or_update_message_counts(message_counts.clone(), tx.stash())
                        .await?;
                }

                if let Some(mail_settings) = event.mail_settings.as_mut() {
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
