use crate::user_context::events::addresses::handle_address_event;
use crate::user_context::events::conversations::handle_conversation_events;
use crate::user_context::events::labels::handle_label_events;
use crate::user_context::events::messages::handle_message_events;
use crate::user_context::events::MailEvent;
use crate::WeakMailUserContext;
use proton_api_mail::proton_api_core::exports::anyhow::anyhow;
use proton_api_mail::proton_api_core::exports::tracing::{debug, error};
use proton_async::async_trait::async_trait;
use proton_event_loop::{Subscriber, SubscriberError};
use proton_mail_db::DBResult;

pub struct MailEventSubscriber(WeakMailUserContext);

impl MailEventSubscriber {
    pub fn new(ctx: WeakMailUserContext) -> Self {
        Self(ctx)
    }
}

#[async_trait]
impl Subscriber<MailEvent> for MailEventSubscriber {
    fn name(&self) -> &str {
        "proton-mail-event-subscriber"
    }
    async fn on_events(&self, events: &[MailEvent]) -> Result<(), SubscriberError> {
        let ctx = self.0.upgrade().ok_or_else(|| {
            let e = anyhow!("MailUserContext no longer alive");
            error!("{e}");
            SubscriberError::Other(e)
        })?;

        let mut conn = ctx.new_db_connection().map_err(|e| {
            let e = anyhow!("Failed to acquire db connection: {e}");
            error!("{e}");
            SubscriberError::Other(e)
        })?;

        conn.tx(|tx| -> DBResult<()> {
            for event in events {
                let event = &event.event;

                if let Some(addresses) = &event.addresses {
                    debug!("Handling address events");
                    handle_address_event(tx, addresses)?;
                }

                if let Some(labels) = &event.labels {
                    debug!("Handling label events");
                    handle_label_events(tx, labels)?;
                }

                if let Some(conversations) = &event.conversations {
                    debug!("Handling conversation events");
                    handle_conversation_events(tx, conversations)?;
                }

                if let Some(messages) = &event.messages {
                    debug!("Handling message events");
                    handle_message_events(tx, messages)?;
                }

                if let Some(conversation_counts) = &event.conversation_counts {
                    debug!("Handling conversation counts");
                    tx.create_or_update_conversation_counts(conversation_counts.iter())?;
                }

                if let Some(message_counts) = &event.message_counts {
                    debug!("Handling message counts");
                    tx.create_or_update_message_counts(message_counts.iter())?;
                }
            }
            Ok(())
        })
        .map_err(|e| {
            let e = anyhow!("Failed to apply changes: {e}");
            error!("{e}");
            SubscriberError::Other(e)
        })
    }
}
