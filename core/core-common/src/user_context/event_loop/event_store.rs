use crate::{CoreEventLoopContext, UserContext};
use anyhow::anyhow;
use async_trait::async_trait;
use proton_core_api::services::proton::EventId;
use proton_event_loop::store::EventStore;
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use tracing::error;

// Once v5 event code has been dropped, these declarations can be moved
// to their respective event source implementations.
pub const CORE_EVENT_TYPE_ID: &str = "proton-core-event";
pub const CONTACT_EVENT_TYPE_ID: &str = "proton-contact-event";
pub const MAIL_EVENT_TYPE_ID: &str = "proton-mail-event";

#[async_trait]
impl EventStore for CoreEventLoopContext {
    async fn load(&self) -> anyhow::Result<Option<proton_event_loop::EventId>> {
        let ctx = self.inner()?;
        load_event_id(&ctx, CORE_EVENT_TYPE_ID).await
    }

    async fn store(&self, id: proton_event_loop::EventId) -> anyhow::Result<()> {
        let ctx = self.inner()?;
        // Start storing the event ids for contacts and mail as well. Backend uses
        // the same cursor internally. When we switch feature on, it will just progress
        // independently.
        ctx.stash()
            .connection()
            .await?
            .tx(async |tx| {
                store_event_id_query(CORE_EVENT_TYPE_ID, id.clone(), tx).await?;
                store_event_id_query(CONTACT_EVENT_TYPE_ID, id.clone(), tx).await?;
                store_event_id_query(MAIL_EVENT_TYPE_ID, id, tx).await
            })
            .await
            .map_err(|e: StashError| {
                error!("Failed to store event id in db:{e:?}");
                anyhow!("Failed to store event id in db:{e:?}")
            })
    }
}

pub async fn load_event_id(
    ctx: &UserContext,
    key: &'static str,
) -> anyhow::Result<Option<proton_event_loop::EventId>> {
    let tether = ctx.stash().connection().await?;
    match load_event_id_query(key, &tether).await {
        Ok(value) => Ok(value.map(|id| id.into_inner().into())),
        Err(e) => {
            error!("Failed to load core event id from db:{e:?}");
            Err(anyhow!("Failed to load core event id {e}"))
        }
    }
}

pub async fn load_event_id_query(
    key: &'static str,
    tether: &Tether,
) -> Result<Option<EventId>, StashError> {
    tether
        .query_value_opt::<EventId>(
            "SELECT value FROM event_id_store WHERE id = ?1",
            params![key],
        )
        .await
}

pub async fn store_event_id(
    ctx: &UserContext,
    key: &'static str,
    id: proton_event_loop::EventId,
) -> anyhow::Result<()> {
    ctx.stash()
        .connection()
        .await?
        .tx(async |tx| store_event_id_query(key, id, tx).await)
        .await
        .map_err(|e: StashError| {
            error!("Failed to store event id ({key}) in db:{e:?}");
            anyhow!("Failed to store event id ({key}) in db:{e:?}")
        })
}

pub async fn store_event_id_query(
    key: &'static str,
    id: proton_event_loop::EventId,
    tx: &Bond<'_>,
) -> Result<(), StashError> {
    tx.execute(
        "INSERT OR REPLACE INTO event_id_store (id, value) VALUES (?, ?)",
        params![key, id.into_inner()],
    )
    .await?;

    Ok(())
}
