use crate::MailUserContext;
use anyhow::anyhow;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

use super::MailActionError;

/// Action which polls the event loop.
///
/// Rather than control exclusive execution access between the queue and the event loop, run
/// the event loop as action in the queue.
#[derive(Serialize, Deserialize)]
pub struct ActionRefresh {}

impl Action for ActionRefresh {
    const TYPE: Type = Type("refresh");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Normal;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = RefreshHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
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
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        context
            .as_arc()
            .on_refresh_impl(255)
            .await
            .map_err(|e| MailActionError::Other(anyhow!("{e}")))?;

        Ok(())
    }
}
