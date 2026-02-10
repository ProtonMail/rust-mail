use crate::event_loop::event_provider::CoreEventProviderError;
use crate::event_loop::v6::CoreEventLoopV6Context;
use async_trait::async_trait;
use proton_core_api::services::proton::ProtonCore;
use proton_event_loop::{EventProvider, EventProviderError, EventProviderResult, RawEvent};

#[async_trait]
impl EventProvider for CoreEventLoopV6Context {
    async fn get_latest_event_id(&self) -> EventProviderResult<proton_event_loop::EventId> {
        async {
            let ctx = self.inner()?;
            Ok::<_, CoreEventProviderError>(
                ctx.session()
                    .get_core_event_latest_v6()
                    .await?
                    .event_id
                    .into_inner()
                    .into(),
            )
        }
        .await
        .map_err(|e| -> Box<dyn EventProviderError> { Box::new(e) })
    }

    async fn get_event(
        &self,
        event_id: &proton_event_loop::EventId,
    ) -> EventProviderResult<RawEvent> {
        async {
            let ctx = self.inner()?;
            let json_string = ctx
                .session()
                .get_core_event_v6(event_id.clone().into_inner().into())
                .await?;

            Ok::<_, CoreEventProviderError>(RawEvent::from_json(json_string)?)
        }
        .await
        .map_err(|e| -> Box<dyn EventProviderError> { Box::new(e) })
    }
}
