use crate::UserContext;
use crate::datatypes::Refresh;
use crate::db::account::CoreAccount;
use crate::events::CoreEvent;
use crate::models::User;
use anyhow::anyhow;
use async_trait::async_trait;
use proton_event_loop::Subscriber;
use proton_event_loop::subscriber::{SubscriberError, SubscriberResult};
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use stash::orm::Model;
use stash::stash::{Bond, StashError};
use std::sync::Weak;
use tracing::{debug, warn};

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct AccountEventSubscriberError(anyhow::Error);

impl SubscriberError for AccountEventSubscriberError {
    fn is_network_failure(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct AccountEventSubscriber(Weak<UserContext>);

impl From<StashError> for AccountEventSubscriberError {
    fn from(err: StashError) -> Self {
        AccountEventSubscriberError(anyhow::Error::new(err))
    }
}

#[async_trait]
impl Subscriber<CoreEvent> for AccountEventSubscriber {
    fn name(&self) -> &'static str {
        "proton-account-event-subscriber"
    }

    #[tracing::instrument(skip(self, events))]
    async fn on_events(&self, events: &mut [CoreEvent]) -> SubscriberResult<()> {
        async {
            let Some(ctx) = self.0.upgrade() else {
                warn!("User context is no longer alive");
                return Ok(());
            };
            for event in events.iter_mut() {
                if let Some(user) = event.user.as_mut() {
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
                            AccountEventSubscriberError(anyhow!("Failed apply changes: {e}"))
                        })?;
                }
            }
            Ok::<_, AccountEventSubscriberError>(())
        }
        .await
        .map_err(|e| -> Box<dyn SubscriberError> { Box::new(e) })
    }

    async fn on_refresh(&self, event: &CoreEvent) -> SubscriberResult<()> {
        async {
            if event.refresh == Refresh::All {
                let Some(ctx) = self.0.upgrade() else {
                    warn!("User context is no longer alive");
                    return Ok(());
                };

                // Note: this relies on the core event subscriber refresh to have completed first
                // since it syncs the data and then we only need to update it
                let user_id = ctx.user_id().clone();
                let user = User::load(user_id.clone(), &ctx.user_stash.connection().await?)
                    .await?
                    .ok_or(AccountEventSubscriberError(anyhow!(
                        "Could not find user with {user_id:?}"
                    )))?;

                ctx.context
                    .account_stash()
                    .connection()
                    .await?
                    .tx::<_, _, AccountEventSubscriberError>(async |tx| {
                        update_account_data(&user, tx).await.map_err(|e| {
                            AccountEventSubscriberError(anyhow!(
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
        .map_err(|e| -> Box<dyn SubscriberError> { Box::new(e) })
    }

    fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

impl From<Weak<UserContext>> for AccountEventSubscriber {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}

async fn update_account_data(user: &User, tx: &Bond<'_>) -> Result<(), StashError> {
    if let Some(account) = CoreAccount::load(user.id(), tx).await? {
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
