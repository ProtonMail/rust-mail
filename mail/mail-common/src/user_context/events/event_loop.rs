use crate::events::MailEvent;
use crate::user_context::events::subscriber::MailEventSubscriber;
use crate::MailUserContext;
use anyhow::anyhow;
use async_trait::async_trait;
use proton_action_queue::queue::ActionError;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::EventId;
use proton_api_core::services::proton::prelude::GetEventOptions;
use proton_api_core::services::proton::ProtonCore;
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::response_data::MailEvent as ApiMailEvent;
use proton_core_common::CoreEventSubscriber;
use proton_event_loop::provider::Provider;
use proton_event_loop::store::Store;
use proton_event_loop::subscriber::Subscriber;
use proton_event_loop::EventLoopError;
use stash::exports::SqliteError;
use stash::params;
use stash::stash::StashError;
use std::sync::Weak;
use tracing::error;

const MAIL_EVENT_TYPE_ID: &str = "proton-mail-event";

#[async_trait]
impl Store for MailUserContext {
    async fn load(&self) -> anyhow::Result<Option<EventId>> {
        let tether = self.user_context.stash().connection();
        match tether
            .query_value::<_, EventId>(
                "SELECT value FROM event_id_store WHERE id = ?1",
                params![MAIL_EVENT_TYPE_ID],
            )
            .await
        {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    error!("Failed to load event id from db:{e:?}");
                    Err(anyhow!("Failed to load event id {e}"))
                }
            }
        }
    }

    async fn store(&self, id: EventId) -> anyhow::Result<()> {
        {
            let mut tether = self.user_context.stash().connection();
            let tx = tether.transaction().await?;
            tx.execute(
                "INSERT OR REPLACE INTO event_id_store (id, value) VALUES (?, ?)",
                params![MAIL_EVENT_TYPE_ID, id],
            )
            .await?;
            tx.commit().await?;

            Ok(())
        }
        .map_err(|e: StashError| {
            error!("Failed to store event id in db:{e:?}");
            anyhow!("Failed to store event id {e}")
        })
    }
}

#[async_trait]
impl Provider<MailEvent> for MailUserContext {
    async fn get_latest_event_id(&self) -> Result<EventId, ApiServiceError> {
        Ok(self.api().get_events_latest().await?.event_id)
    }

    async fn get_event(&self, event_id: &EventId) -> Result<MailEvent, ApiServiceError> {
        Ok(self
            .session()
            .api()
            .get_event::<ApiMailEvent>(event_id.clone(), GetEventOptions::all())
            .await?
            .into())
    }
}

impl MailUserContext {
    /// Queue an action to execute the event loop.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    pub async fn poll_event_loop(
        &self,
    ) -> Result<(), ActionError<crate::actions::event_poll::EventPoll>> {
        self.action_queue()
            .queue_action(crate::actions::event_poll::EventPoll {})
            .await?;
        Ok(())
    }

    /// Perform one iteration of the event loop, which consists of retrieving the latest events,
    /// publishing it on all the registered subscribers and storing the event id for the next
    /// iteration.
    ///
    /// The execution of the loop is aborted on the first error.
    ///
    /// # Error
    ///
    /// Returns error if the event loop failed to poll.
    pub(crate) async fn poll_event_loop_impl(&self) -> Result<(), EventLoopError> {
        let core_subscriber = CoreEventSubscriber::new(Weak::clone(&self.this));
        let mail_subscriber = MailEventSubscriber::new(Weak::clone(&self.this));
        let subscribers: [Box<dyn Subscriber<MailEvent>>; 2] =
            [Box::new(core_subscriber), Box::new(mail_subscriber)];

        self.event_loop.poll(self, self, &subscribers).await
    }
}
