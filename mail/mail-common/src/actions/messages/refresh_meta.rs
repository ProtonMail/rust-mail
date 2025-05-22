use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use proton_core_common::models::ModelExtension;
use proton_mail_ids::LocalMessageId;
use serde::{self, Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;

/// Prefetch message body action.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RefreshMeta {
    local_id: LocalMessageId,
}

impl RefreshMeta {
    /// Create new instance.
    pub fn new(local_id: LocalMessageId) -> Self {
        Self { local_id }
    }
}

impl Action for RefreshMeta {
    const TYPE: Type = Type("prefetch_message_metadata");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Lowest;
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
    type Action = RefreshMeta;

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
        let message = Message::load(action.local_id, guard.tether())
            .await?
            .filter(|msg| !msg.is_draft() && msg.remote_id.is_some());

        if let Some(message) = message {
            let remote_id = message.remote_id.unwrap();
            let items = Message::sync_metadata(vec![remote_id], ctx.api(), &mut guard).await?;
            if items.is_empty() {
                // The message appears to be not found remotely, delete it.
                tracing::warn!(
                    "While prefetchin message metadata found a local message without remote counterpart. Deleteing."
                );
                guard
                    .tx(async |tx| {
                        Message::delete_by_id(action.local_id, tx).await?;
                        Result::<(), <Self::Action as Action>::Error>::Ok(())
                    })
                    .await?;
            }
        }

        Ok(())
    }
}
