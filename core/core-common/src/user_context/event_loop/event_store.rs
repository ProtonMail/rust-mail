use crate::CoreEventLoopContext;
use anyhow::anyhow;
use async_trait::async_trait;
use proton_core_api::services::proton::EventId;
use proton_event_loop::store::EventStore;
use stash::params;
use stash::stash::StashError;
use tracing::error;

const CORE_EVENT_TYPE_ID: &str = "proton-core-event";

#[async_trait]
impl EventStore for CoreEventLoopContext {
    async fn load(&self) -> anyhow::Result<Option<proton_event_loop::EventId>> {
        let ctx = self.inner()?;
        let tether = ctx.stash().connection().await?;
        match tether
            .query_value_opt::<EventId>(
                "SELECT value FROM event_id_store WHERE id = ?1",
                params![CORE_EVENT_TYPE_ID],
            )
            .await
        {
            Ok(value) => Ok(value.map(|id| id.into_inner().into())),
            Err(e) => {
                error!("Failed to load core event id from db:{e:?}");
                Err(anyhow!("Failed to load core event id {e}"))
            }
        }
    }

    async fn store(&self, id: proton_event_loop::EventId) -> anyhow::Result<()> {
        let ctx = self.inner()?;
        ctx.stash()
            .connection()
            .await?
            .tx(async |tx| {
                tx.execute(
                    "INSERT OR REPLACE INTO event_id_store (id, value) VALUES (?, ?)",
                    params![CORE_EVENT_TYPE_ID, id.into_inner()],
                )
                .await?;

                Ok(())
            })
            .await
            .map_err(|e: StashError| {
                error!("Failed to store core event id in db:{e:?}");
                anyhow!("Failed to store core event id {e}")
            })
    }
}
