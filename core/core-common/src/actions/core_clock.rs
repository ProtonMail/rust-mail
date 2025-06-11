use std::time::Duration;

use crate::{Context as CoreContext, CoreContextError, models::AppSettings};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

/// Action which polls the event loop.
///
/// Rather than control exclusive execution access between the queue and the event loop, run
/// the event loop as action in the queue.
#[derive(Serialize, Deserialize)]
pub struct CoreClock {
    interval: Duration,
}

impl CoreClock {
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self { interval }
    }
}

impl Action for CoreClock {
    const TYPE: Type = Type("core_clock");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::High;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = CoreClockHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = CoreContextError;
    type Context = CoreContext;
}

#[derive(Default)]
pub struct CoreClockHandler;

impl proton_action_queue::action::Handler for CoreClockHandler {
    type Action = CoreClock;
    type Context = CoreContext;

    async fn apply_local(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let mut tether = ctx.account_stash().connection();
        let mut app_settings = AppSettings::get_or_default(&tether).await;
        app_settings.touch(&mut tether, action.interval).await?;

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
        _: &Self::Context,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
    }
}
