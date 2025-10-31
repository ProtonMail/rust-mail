use crate::actions::{MailActionError, PREFETCH_ROLLBACK_ACTION_GROUP};
use crate::datatypes::{ConversationViewOptions, LocalConversationId};
use crate::models::{Conversation, Message};
use crate::{MailContextError, MailUserContext};
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionGroup, ActionId, DefaultVersionConverter, Handler,
    Priority, Type, WriterGuard,
};
use proton_core_common::actions::dependency_builder::ActionDependencyKeysBuilder;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label;
use serde::{self, Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use std::sync::Weak;
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Prefetch {
    local_id: LocalConversationId,
    local_label_id: LocalLabelId,
}

impl Prefetch {
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
    const PRIORITY: Priority = Priority::Lowest;

    const GROUP: ActionGroup = PREFETCH_ROLLBACK_ACTION_GROUP;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = PrefetchHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeysBuilder::default().build()
    }
}

pub struct PrefetchHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler for PrefetchHandler {
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
        tracing::trace!("Prefetching {:?}", action.local_id);

        let ctx = self.ctx.upgrade().ok_or(MailActionError::LostContext)?;
        let deleted = Conversation::is_deleted(action.local_id, guard.tether()).await?;

        if deleted {
            tracing::debug!(
                "Conversation is deleted, skipping prefetch action, conversation_id: `{}`",
                action.local_id
            );
            return Ok(());
        }

        let _ = Conversation::sync_conversation_messages(
            ctx.network_monitor_service(),
            action.local_id,
            &mut guard,
            ctx.session(),
            false,
            ctx.action_queue(),
        )
        .await;

        let messages = Message::in_conversation(
            action.local_id,
            ConversationViewOptions::All,
            guard.tether(),
        )
        .await?;

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

        tracing::trace!(
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

        if let Err(e) = local_message.fetch_message_body(&ctx, &mut guard).await {
            match e {
                MailContextError::Api(network_error) => {
                    return Err(MailActionError::Http(network_error));
                }
                _ => {
                    error!("Error prefetching message body, details: `{e}`");
                }
            }
        }

        Ok(())
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
