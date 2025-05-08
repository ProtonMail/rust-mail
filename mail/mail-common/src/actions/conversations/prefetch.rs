use crate::MailUserContext;
use crate::actions::MailActionError;
use crate::models::{Conversation, Message};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use proton_mail_ids::LocalConversationId;
use serde::{self, Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use tracing::error;

/// Prefetch conversation data action.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Prefetch {
    local_id: LocalConversationId,
    local_label_id: LocalLabelId,
}

impl Prefetch {
    /// Create new instance.
    pub fn new(local_id: LocalConversationId, local_label_id: LocalLabelId) -> Self {
        Self {
            local_id,
            local_label_id,
        }
    }
}

impl Action for Prefetch {
    const TYPE: Type = Type("prefetch_conversation");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Low;
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
        let session = ctx.session();
        let _ =
            Conversation::sync_conversation_messages(action.local_id, &mut guard, session).await;
        let messages = Message::in_conversation(action.local_id, guard.tether()).await?;
        let Some(label) = Label::load(action.local_label_id, guard.tether()).await? else {
            error!(
                "Label not found for prefetch action, label_id: `{}`",
                action.local_label_id
            );
            return Ok(());
        };
        let Ok(message_id_to_open) =
            Conversation::message_id_to_open(action.local_id, &label, &messages)
        else {
            error!(
                "Message id to open was not found for prefetch action, conversation_id: `{}`",
                action.local_id
            );
            return Ok(());
        };
        tracing::debug!(
            "Prefetching message {message_id_to_open} body for conversation `{local_id}`",
            local_id = action.local_id
        );
        let Some(local_message) = Message::load(message_id_to_open, guard.tether()).await? else {
            error!(
                "Message not found for prefetch action, conversation_id: `{}`",
                action.local_id
            );
            return Ok(());
        };

        if let Err(e) = local_message.fetch_message_body(ctx, &mut guard).await {
            tracing::error!("Couldn't prefetch message body, details: `{e}`");
        };

        Ok(())
    }
}
