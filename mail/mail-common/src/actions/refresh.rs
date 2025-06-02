use crate::{MailUserContext, actions::event_poll::ActionEventLoopError};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use proton_core_common::datatypes::Refresh;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

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
    type Handler = RefreshHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = ActionEventLoopError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct RefreshHandler;

impl proton_action_queue::action::Handler for RefreshHandler {
    type Action = ActionRefresh;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        // Nothing to do.
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        context: &Self::Context,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        context
            .arc_user_context()
            .on_refresh_impl(action.refresh)
            .await?;
        context.as_arc().on_refresh_impl(action.refresh).await?;

        Ok(())
    }
}
