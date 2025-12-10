use crate::events::v6::MailEventLoopV6Context;
use async_trait::async_trait;
use proton_core_api::service::ApiServiceError;
use proton_event_loop::{EventProvider, EventProviderError, EventProviderResult, RawEvent};
use proton_mail_api::services::proton::ProtonMail;

#[derive(Debug, thiserror::Error)]
pub enum MailEventProviderError {
    #[error(transparent)]
    Api(#[from] ApiServiceError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl EventProviderError for MailEventProviderError {
    fn is_network_failure(&self) -> bool {
        match self {
            MailEventProviderError::Api(e) => e.is_network_failure(),
            MailEventProviderError::Other(_) => false,
        }
    }
}

#[async_trait]
impl EventProvider for MailEventLoopV6Context {
    async fn get_latest_event_id(&self) -> EventProviderResult<proton_event_loop::EventId> {
        async {
            let ctx = self.inner()?;
            Ok::<_, MailEventProviderError>(
                ctx.session()
                    .get_mail_event_latest_v6()
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
                .get_mail_event_v6(event_id.clone().into_inner().into())
                .await?;

            Ok::<_, MailEventProviderError>(RawEvent::from_json(json_string)?)
        }
        .await
        .map_err(|e| -> Box<dyn EventProviderError> { Box::new(e) })
    }
}
