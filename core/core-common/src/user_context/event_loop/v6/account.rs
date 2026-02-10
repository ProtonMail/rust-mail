use crate::UserContext;
use crate::db::account::CoreAccount;
use crate::event_loop::account_event_subscriber::AccountEventSubscriberError;
use crate::event_loop::v6::CoreEventSourceV6;
use anyhow::anyhow;
use async_trait::async_trait;
use proton_core_api::services::proton::User;
use proton_event_loop::v6::{EventSource, EventSubscriber};
use proton_event_loop::{EventSubscriberError, EventSubscriberResult, RefreshFlag};
use proton_issue_reporter_service::{IssueLevel, issue_report_keys_from_error};
use stash::AccountDb;
use stash::orm::Model;
use stash::stash::{Bond, StashError};
use std::sync::Weak;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct AccountEventV6Subscriber(Weak<UserContext>);

impl From<Weak<UserContext>> for AccountEventV6Subscriber {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}

#[async_trait]
impl EventSubscriber<CoreEventSourceV6> for AccountEventV6Subscriber {
    fn name(&self) -> &'static str {
        "account-v6-event-subscriber"
    }

    async fn on_event(
        &self,
        event: &<CoreEventSourceV6 as EventSource>::Event,
        cache: &mut <CoreEventSourceV6 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("User context is no longer alive");
            return Ok(());
        };

        async {
            if event.users.as_ref().is_some_and(|v| !v.is_empty()) {
                debug!("Handling account user event");
                let user = cache.get_or_fetch_user(ctx.session()).await?;
                handle_user_update_event(&ctx, user).await?;
            }
            Ok::<_, AccountEventSubscriberError>(())
        }
        .await
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }

    async fn on_refresh(
        &self,
        _: RefreshFlag,
        cache: &mut <CoreEventSourceV6 as EventSource>::Cache,
    ) -> EventSubscriberResult<()> {
        let Some(ctx) = self.0.upgrade() else {
            warn!("User context is no longer alive");
            return Ok(());
        };

        async {
            let user = cache.get_or_fetch_user(ctx.session()).await?;

            handle_user_refresh(&ctx, user).await
        }
        .await
        .map_err(|e| -> Box<dyn EventSubscriberError> { Box::new(e) })
    }
}

pub(crate) async fn handle_user_update_event(
    ctx: &UserContext,
    user: &User,
) -> Result<(), AccountEventSubscriberError> {
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
        })
}

pub(crate) async fn handle_user_refresh(
    ctx: &UserContext,
    user: &User,
) -> Result<(), AccountEventSubscriberError> {
    ctx.context
        .account_stash()
        .connection()
        .await?
        .tx::<_, _, AccountEventSubscriberError>(async |tx| {
            update_account_data(user, tx).await.map_err(|e| {
                AccountEventSubscriberError::Other(anyhow!("Failed apply refresh changes: {e}"))
            })
        })
        .await
        .inspect_err(|e| {
            ctx.issue_reporter_service().report(
                IssueLevel::Critical,
                "Failed to apply account refresh event".into(),
                issue_report_keys_from_error(e),
            );
        })
}

async fn update_account_data(user: &User, tx: &Bond<'_, AccountDb>) -> Result<(), StashError> {
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
