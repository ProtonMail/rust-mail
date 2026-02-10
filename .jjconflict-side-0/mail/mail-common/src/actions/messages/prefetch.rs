use crate::actions::{MailActionError, PREFETCH_ROLLBACK_ACTION_GROUP};
use crate::datatypes::LocalMessageId;
use crate::models::Message;
use crate::{MailContextError, MailUserContext};
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionGroup, ActionId, DefaultVersionConverter, Handler,
    Priority, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use serde::{self, Deserialize, Serialize};
use stash::UserDb;
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

impl Action<UserDb> for Prefetch {
    const TYPE: Type = Type("prefetch_message");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Lowest;
    const GROUP: ActionGroup = PREFETCH_ROLLBACK_ACTION_GROUP;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = PrefetchHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        ActionDependencyKeys::default()
    }
}

pub struct PrefetchHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler<UserDb> for PrefetchHandler {
    type Action = Prefetch;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<
        <Self::Action as Action<UserDb>>::LocalOutput,
        <Self::Action as Action<UserDb>>::Error,
    > {
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_, UserDb>,
    ) -> Result<
        <Self::Action as Action<UserDb>>::RemoteOutput,
        <Self::Action as Action<UserDb>>::Error,
    > {
        tracing::trace!(
            "Prefetching message {local_id} body",
            local_id = action.local_id
        );

        let ctx = self.ctx.upgrade().ok_or(MailActionError::LostContext)?;

        let Some(local_message) = Message::load(action.local_id, guard.tether()).await? else {
            error!(
                "Message not found for prefetch action, message_id: `{}`",
                action.local_id
            );

            return Ok(());
        };

        if local_message.deleted {
            tracing::debug!(
                "Message is deleted, skipping prefetch action, message_id: `{}`",
                action.local_id
            );
            return Ok(());
        }

        // Check if message is in deleted_items tombstone table
        if let Some(ref remote_id) = local_message.remote_id {
            use crate::datatypes::DeletedItemType;
            use crate::models::DeletedItem;

            let deleted_tombstones = DeletedItem::find_deleted_by_remote_ids(
                std::iter::once(remote_id.as_str()),
                DeletedItemType::Message,
                guard.tether(),
            )
            .await?;

            if !deleted_tombstones.is_empty() {
                tracing::debug!(
                    "Message is in deleted_items, skipping prefetch action, message_id: `{}`",
                    action.local_id
                );
                return Ok(());
            }
        }

        if let Err(e) = local_message.prefetch_message_body(&ctx, &mut guard).await {
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
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Ok(())
    }
}
