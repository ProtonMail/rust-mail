use crate::event_loop::event_store::CORE_EVENT_TYPE_ID;
use crate::event_loop::v6::CoreEventLoopV6Context;
use crate::user_context::event_loop::event_store;
use async_trait::async_trait;
use proton_event_loop::store::EventStore;

#[async_trait]
impl EventStore for CoreEventLoopV6Context {
    async fn load(&self) -> anyhow::Result<Option<proton_event_loop::EventId>> {
        let ctx = self.inner()?;
        event_store::load_event_id(&ctx, CORE_EVENT_TYPE_ID).await
    }

    async fn store(&self, id: proton_event_loop::EventId) -> anyhow::Result<()> {
        let ctx = self.inner()?;
        event_store::store_event_id(&ctx, CORE_EVENT_TYPE_ID, id).await
    }
}
