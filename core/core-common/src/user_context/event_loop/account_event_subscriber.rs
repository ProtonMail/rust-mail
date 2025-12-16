use crate::UserContext;
use crate::datatypes::Refresh;
use crate::event_loop::event_source::CoreEventSource;
use crate::event_loop::v6::{CoreEventCache, handle_user_refresh, handle_user_update_event};
use async_trait::async_trait;
use proton_core_api::service::ApiServiceError;
use proton_event_loop::RefreshFlag;
use proton_event_loop::v6::{
    EventSource, EventSubscriber, EventSubscriberError, EventSubscriberResult,
};
use stash::stash::StashError;
use std::sync::Weak;
use tracing::{debug, warn};

#[derive(Debug, thiserror::Error)]
pub enum AccountEventSubscriberError {
    #[error(transparent)]
    Api(#[from] ApiServiceError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl EventSubscriberError for AccountEventSubscriberError {
    fn is_network_failure(&self) -> bool {
        match self {
            AccountEventSubscriberError::Api(e) => e.is_network_failure(),
            AccountEventSubscriberError::Other(_) => false,
        }
    }

    fn is_retryable(&self) -> bool {
        match self {
            AccountEventSubscriberError::Api(e) => e.is_network_failure() || e.is_server_failure(),
            AccountEventSubscriberError::Other(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct AccountEventSubscriber(Weak<UserContext>);

impl From<StashError> for AccountEventSubscriberError {
    fn from(err: StashError) -> Self {
        AccountEventSubscriberError::Other(anyhow::Error::new(err))
    }
}

#[async_trait]
impl EventSubscriber<CoreEventSource> for AccountEventSubscriber {
    fn name(&self) -> &'static str {
        "account-event-subscriber"
    }

    #[tracing::instrument(skip_all)]
    async fn on_event(
        &self,
        event: &<CoreEventSource as EventSource>::Event,
        _: &mut CoreEventCache,
    ) -> EventSubscriberResult<()> {
        async {
            let Some(ctx) = self.0.upgrade() else {
                warn!("User context is no longer alive");
                return Ok(());
            };
            if let Some(user) = event.user.as_ref() {
                debug!("Handling account user event");

                handle_user_update_event(&ctx, user).await?;
            }
            Ok::<_, AccountEventSubscriberError>(())
        }
        .await
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }

    async fn on_refresh(
        &self,
        refresh_flag: RefreshFlag,
        cache: &mut CoreEventCache,
    ) -> EventSubscriberResult<()> {
        async {
            if Refresh::from(refresh_flag.as_u8()) == Refresh::All {
                let Some(ctx) = self.0.upgrade() else {
                    warn!("User context is no longer alive");
                    return Ok(());
                };

                let user = cache.get_or_fetch_user(ctx.session()).await?;

                handle_user_refresh(&ctx, user).await?;
            }
            Ok::<_, AccountEventSubscriberError>(())
        }
        .await
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }
}

impl From<Weak<UserContext>> for AccountEventSubscriber {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}
