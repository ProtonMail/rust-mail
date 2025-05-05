use crate::actions::MailActionError;
use crate::models::Message;
use crate::{AppError, MailUserContext};
use proton_action_queue::action::{Action, ActionId, DefaultVersionConverter, Type, WriterGuard};
use proton_mail_ids::LocalMessageId;
use serde::{self, Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;

/// Prefetch conversation data action.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Prefetch {
    local_id: LocalMessageId,
}

impl Prefetch {
    /// Create new instance.
    pub fn new(local_id: LocalMessageId) -> Self {
        Self { local_id }
    }
}

impl Action for Prefetch {
    const TYPE: Type = Type("prefetch_message");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailActionError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Prefetch;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        tracing::debug!(
            "Prefetching message {local_id} body",
            local_id = action.local_id
        );

        let saved_message = Message::load(action.local_id, guard.tether())
            .await?
            .ok_or(AppError::MessageMissing(action.local_id))?;

        if let Err(e) = saved_message.fetch_message_body(ctx, &mut guard).await {
            tracing::error!("Couldn't prefetch message body, details: `{e}`");
        };

        Ok(())
    }
}
