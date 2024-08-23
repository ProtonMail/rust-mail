use crate::events::MailEvent;
use crate::user_context::events::subscriber::MailEventSubscriber;
use crate::MailUserContext;
use anyhow::anyhow;
use async_trait::async_trait;
use futures::executor::block_on;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::response_data::MailEvent as ApiMailEvent;
use proton_core_common::datatypes::RemoteId;
use proton_core_common::CoreEventSubscriber;
use proton_event_loop::provider::Provider;
use proton_event_loop::store::Store;
use proton_event_loop::subscriber::Subscriber;
use proton_event_loop::EventLoopError;
use stash::exports::SqliteError;
use stash::params;
use stash::stash::{Interface, StashError};
use std::sync::Weak;
use tracing::error;

const MAIL_EVENT_TYPE_ID: &str = "proton-mail-event";

impl Store for MailUserContext {
    fn load(&self) -> anyhow::Result<Option<ApiRemoteId>> {
        let conn = self.user_context.stash();
        match block_on(async {
            conn.query_value::<_, String>(
                "SELECT value FROM event_id_store WHERE id = ?1",
                params![MAIL_EVENT_TYPE_ID],
            )
            .await
        })
        .map(ApiRemoteId::from)
        {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    error!("Failed to load event id from db:{e}");
                    Err(anyhow!("Failed to load event id {e}"))
                }
            }
        }
    }

    fn store(&self, id: ApiRemoteId) -> anyhow::Result<()> {
        let conn = self.user_context.stash();
        block_on(async {
            conn.execute(
                "INSERT OR REPLACE INTO event_id_store (id, value) VALUES (?, ?)",
                params![MAIL_EVENT_TYPE_ID, RemoteId::from(id)],
            )
            .await
            .map_err(|e| {
                error!("Failed to store event id in db:{e}");
                anyhow!("Failed to store event id {e}")
            })
        })
        .map(|_| ())
    }
}

#[async_trait]
impl Provider<MailEvent> for MailUserContext {
    async fn get_latest_event_id(&self) -> Result<ApiRemoteId, ApiServiceError> {
        Ok(self.session().api().get_events_latest().await?.event_id)
    }

    async fn get_event(&self, event_id: &ApiRemoteId) -> Result<MailEvent, ApiServiceError> {
        Ok(self
            .session()
            .api()
            .get_event::<ApiMailEvent>(event_id.clone(), true, true)
            .await?
            .into())
    }
}

impl MailUserContext {
    pub async fn poll_event_loop(&self) -> Result<(), EventLoopError> {
        let core_subscriber = CoreEventSubscriber::new(Weak::clone(&self.this));
        let mail_subscriber = MailEventSubscriber::new(Weak::clone(&self.this));
        //TODO: better way to store this.
        // TODO: Temporarily disabled core events here - the new event handler will
        // TODO: deal with all of this
        let subscribers: [Box<dyn Subscriber<MailEvent>>; 2] =
            [Box::new(core_subscriber), Box::new(mail_subscriber)];
        // let subscribers: [Box<dyn Subscriber<MailEvent>>; 1] = [Box::new(mail_subscriber)];
        self.event_loop.poll(self, self, &subscribers).await
    }
}
