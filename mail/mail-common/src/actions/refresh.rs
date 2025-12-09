use crate::MailUserContext;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_common::actions::event_poll::ActionEventLoopError;
use proton_core_common::datatypes::Refresh;
use proton_event_loop::MAX_ERROR_RETRIES;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::sync::Weak;

/// Action which runs whole refresh simulating Subscriber::on_refresh for Resync of eventloop.
///
#[derive(Serialize, Deserialize)]
pub struct ActionRefresh {
    refresh: Refresh,
}

impl ActionRefresh {
    pub fn new(refresh: Refresh) -> Self {
        Self { refresh }
    }
}

impl Action for ActionRefresh {
    const TYPE: Type = Type("refresh");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Normal;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ActionRefreshHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = ActionEventLoopError;
}

pub struct ActionRefreshHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler for ActionRefreshHandler {
    type Action = ActionRefresh;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let ctx = self
            .ctx
            .upgrade()
            .ok_or(ActionEventLoopError::LostContext)?;

        let mut num_attempts = 0;
        tracing::info!("Refreshing core");
        while let Err(e) = ctx.user_context().on_refresh_impl(action.refresh).await {
            num_attempts += 1;
            tracing::error!("Error refreshing core (attempt={num_attempts}): {e}");
            if !e.is_retryable() || num_attempts == MAX_ERROR_RETRIES {
                return Err(e.into());
            }
        }

        tracing::info!("Refreshing mail");
        let mut num_attempts = 0;
        while let Err(e) = ctx.on_refresh_impl(action.refresh).await {
            num_attempts += 1;
            tracing::error!("Error refreshing mail (attempt={num_attempts}): {e}");
            if !e.is_retryable() || num_attempts == MAX_ERROR_RETRIES {
                return Err(e.into());
            }
        }

        Ok(())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
