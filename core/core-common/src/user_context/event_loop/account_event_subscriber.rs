use crate::UserContext;
use crate::datatypes::Refresh;
use crate::db::account::CoreAccount;
use crate::event_loop::event_source::{CoreEventCache, CoreEventSource};
use anyhow::anyhow;
use async_trait::async_trait;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::User as ApiUser;
use proton_event_loop::v6::{
    EventSource, EventSubscriber, EventSubscriberError, EventSubscriberResult,
};
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use stash::orm::Model;
use stash::stash::{Bond, StashError};
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
                // Update CoreAccount table:
                ctx.context
                    .account_stash()
                    .connection()
                    .await?
                    .tx::<_, _, StashError>(async |tx| update_account_data(user, tx).await)
                    .await
                    .map_err(|e: StashError| {
                        ctx.issue_reporter_service().report(
                            IssueLevel::Critical,
                            "Failed to apply account event".into(),
                            issue_report_keys_from_error(&e),
                        );
                        AccountEventSubscriberError::Other(anyhow!("Failed apply changes: {e}"))
                    })?;
            }
            Ok::<_, AccountEventSubscriberError>(())
        }
        .await
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }

    async fn on_refresh<'a>(
        &self,
        event: Option<&'a <CoreEventSource as EventSource>::Event>,
        cache: &mut CoreEventCache,
    ) -> EventSubscriberResult<()> {
        async {
            if event.is_none_or(|event| Refresh::from(event.refresh) == Refresh::All) {
                let Some(ctx) = self.0.upgrade() else {
                    warn!("User context is no longer alive");
                    return Ok(());
                };

                let user = cache.get_or_fetch_user(ctx.session()).await?;

                ctx.context
                    .account_stash()
                    .connection()
                    .await?
                    .tx::<_, _, AccountEventSubscriberError>(async |tx| {
                        update_account_data(user, tx).await.map_err(|e| {
                            AccountEventSubscriberError::Other(anyhow!(
                                "Failed apply refresh changes: {e}"
                            ))
                        })
                    })
                    .await
                    .inspect_err(|e| {
                        ctx.issue_reporter_service().report(
                            IssueLevel::Critical,
                            "Failed to apply account refresh event".into(),
                            issue_report_keys_from_error(e),
                        );
                    })?;
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

async fn update_account_data(user: &ApiUser, tx: &Bond<'_>) -> Result<(), StashError> {
    if let Some(account) = CoreAccount::load(user.id.clone(), tx).await? {
        account
            .with_display_name(user.display_name.clone().unwrap_or_default())
            .with_name_or_addr(user.name.clone().unwrap_or_else(|| user.email.clone()))
            .with_primary_addr(user.email.clone())
            .with_username(user.name.clone().unwrap_or_default())
            .save(tx)
            .await
    } else {
        Ok(())
    }
}
