use crate::events::MailEvent;
use crate::MailUserContext;
use anyhow::anyhow;
use async_trait::async_trait;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::prelude::GetEventOptions;
use proton_api_core::services::proton::ProtonCore;
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::response_data::MailEvent as ApiMailEvent;
use proton_core_common::datatypes::RemoteId;
use proton_event_loop::provider::Provider;
use proton_event_loop::store::Store;
use proton_event_loop::EventLoopError;
use stash::exports::SqliteError;
use stash::params;
use stash::stash::StashError;
use tracing::error;

const MAIL_EVENT_TYPE_ID: &str = "proton-mail-event";

#[async_trait]
impl Store for MailUserContext {
    async fn load(&self) -> anyhow::Result<Option<ApiRemoteId>> {
        let tether = self.user_context.stash().connection();
        match {
            tether
                .query_value::<_, String>(
                    "SELECT value FROM event_id_store WHERE id = ?1",
                    params![MAIL_EVENT_TYPE_ID],
                )
                .await
        }
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

    async fn store(&self, id: ApiRemoteId) -> anyhow::Result<()> {
        {
            let mut tether = self.user_context.stash().connection();
            let tx = tether.transaction().await?;
            tx.execute(
                "INSERT OR REPLACE INTO event_id_store (id, value) VALUES (?, ?)",
                params![MAIL_EVENT_TYPE_ID, RemoteId::from(id)],
            )
            .await?;
            tx.commit().await?;

            Ok(())
        }
        .map_err(|e: StashError| {
            error!("Failed to store event id in db:{e}");
            anyhow!("Failed to store event id {e}")
        })
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
            .get_event::<ApiMailEvent>(event_id.clone(), GetEventOptions::all())
            .await?
            .into())
    }
}

impl MailUserContext {
    pub async fn poll_event_loop(&self) -> Result<(), EventLoopError> {
        self.exclusive.poll_event_loop(self).await
    }
}
