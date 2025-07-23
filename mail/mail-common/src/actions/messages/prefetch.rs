use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::datatypes::LocalMessageId;
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use serde::{self, Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use std::sync::Weak;
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Prefetch {
    local_id: LocalMessageId,
}

impl Prefetch {
    pub fn new(local_id: LocalMessageId) -> Self {
        Self { local_id }
    }
}

impl Action for Prefetch {
    const TYPE: Type = Type("prefetch_message");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Lowest;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = PrefetchHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct PrefetchHandler {
    pub ctx: Weak<MailUserContext>,
}

impl proton_action_queue::action::Handler for PrefetchHandler {
    type Action = Prefetch;

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
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        tracing::debug!(
            "Prefetching message {local_id} body",
            local_id = action.local_id
        );

        let ctx = self.ctx.upgrade().expect("context has died");

        let Some(local_message) = Message::load(action.local_id, guard.tether()).await? else {
            error!(
                "Message not found for prefetch action, message_id: `{}`",
                action.local_id
            );

            return Ok(());
        };

        if let Err(e) = local_message.fetch_message_body(&ctx, &mut guard).await {
            tracing::error!("Couldn't prefetch message body, details: `{e}`");
        };

        Ok(())
    }
}
