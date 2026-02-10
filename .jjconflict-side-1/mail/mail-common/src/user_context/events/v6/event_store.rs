use crate::events::v6::MailEventLoopV6Context;
use async_trait::async_trait;
use proton_core_common::event_loop::event_store;
use proton_core_common::event_loop::event_store::MAIL_EVENT_TYPE_ID;
use proton_event_loop::store::EventStore;

#[async_trait]
impl EventStore for MailEventLoopV6Context {
    async fn load(&self) -> anyhow::Result<Option<proton_event_loop::EventId>> {
        let ctx = self.inner()?;
        event_store::load_event_id(&ctx.user_context, MAIL_EVENT_TYPE_ID).await
    }

    async fn store(&self, id: proton_event_loop::EventId) -> anyhow::Result<()> {
        let ctx = self.inner()?;
        event_store::store_event_id(&ctx.user_context, MAIL_EVENT_TYPE_ID, id).await
    }
}
