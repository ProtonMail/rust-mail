use crate::actions::MailActionError;
use crate::models::{Message, MessageScrollData};
use crate::{AppError, MailUserContext};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use proton_core_common::models::ModelExtension;
use proton_mail_ids::LocalMessageId;
use serde::{self, Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;

/// Refresh message metadata action.
///
/// This action is designed to refresh existing metadata
/// in a case of suspicion that this message may be outdated.
///
/// On failure the local message will be removed.
///
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RefreshMetadata {
    local_id: LocalMessageId,
}

impl RefreshMetadata {
    pub fn new(local_id: LocalMessageId) -> Self {
        Self { local_id }
    }
}

impl Action for RefreshMetadata {
    const TYPE: Type = Type("refresh_message_metadata");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Normal;
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
    type Action = RefreshMetadata;

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
            let items_sync_result =
                Message::sync_metadata(vec![remote_id], ctx.api(), &mut guard).await;
            match items_sync_result {
                Ok(items) if items.is_empty() => {
                    // The message appears to be not found remotely, delete it.
                    tracing::warn!(
                        "Local message without remote counterpart found while refreshing. Deleteing."
                    );
                    guard
                        .tx(async |tx| {
                            Message::delete_by_id(action.local_id, tx).await?;
                            Result::<(), <Self::Action as Action>::Error>::Ok(())
                        })
                        .await?;
                }
                Ok(_) => (),
                Err(AppError::API(e)) if e.is_network_failure() => {
                    return Err(MailActionError::Http(e));
                }
                Err(e) => {
                    tracing::error!("Unexpected error while refreshing message metadata: `{e}`");
                    tracing::error!("Deleting local message: `{}`", action.local_id);
                    guard
                        .tx(async |tx| {
                            Message::delete_by_id(action.local_id, tx).await?;
                            MessageScrollData::delete_all(tx).await?;
                            Result::<(), <Self::Action as Action>::Error>::Ok(())
                        })
                        .await?;

                    return Err(e.into());
                }
            }
        }

        Ok(())
    }
}
