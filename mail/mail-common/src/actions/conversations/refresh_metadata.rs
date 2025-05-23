use anyhow::anyhow;
use std::collections::HashMap;

use crate::AppError;
use crate::actions::MailActionError;
use crate::models::{Conversation, ConversationScrollData};
use crate::{MailUserContext, models::Message};
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::prelude::GetMessagesOptions;
use proton_mail_ids::LocalConversationId;
use proton_task_service::AsyncTaskResult;
use serde::{self, Deserialize, Serialize};
use stash::stash::Bond;

/// Refresh conversation metadata action.
///
/// This action is designed to refresh existing metadata
/// in a case of suspicion that this conversation may be outdated.
///
/// On failure the local conversation will be removed.
///
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RefreshMetadata {
    local_id: LocalConversationId,
}

impl RefreshMetadata {
    pub fn new(local_id: LocalConversationId) -> Self {
        Self { local_id }
    }
}

impl Action for RefreshMetadata {
    const TYPE: Type = Type("refresh_conversation_metadata");
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
        if let Some(remote_id) =
            Conversation::local_id_counterpart(action.local_id, guard.tether()).await?
        {
            let items_sync_result =
                Conversation::sync_metadata(vec![remote_id.clone()], ctx.api(), &mut guard).await;
            let items = match items_sync_result {
                Ok(items) => items,
                Err(AppError::API(e)) if e.is_network_failure() => {
                    return Err(MailActionError::Http(e));
                }
                Err(e) => {
                    tracing::error!(
                        "Unexpected error while refreshing conversation metadata: `{e}`"
                    );
                    tracing::error!("Deleting local conversation: `{}`", action.local_id);
                    guard
                        .tx(async |tx| {
                            Conversation::delete_by_id(action.local_id, tx).await?;
                            Result::<(), <Self::Action as Action>::Error>::Ok(())
                        })
                        .await?;

                    return Err(e.into());
                }
            };

            if items.is_empty() {
                // The conversation appears to be not found remotely, delete it.
                tracing::warn!(
                    "While prefetchin conversation metadata found a local conversation without remote counterpart. Deleteing."
                );
                guard
                    .tx(async |tx| {
                        Conversation::delete_by_id(action.local_id, tx).await?;
                        ConversationScrollData::delete_all(tx).await?;
                        Result::<(), <Self::Action as Action>::Error>::Ok(())
                    })
                    .await?;
            } else {
                let conv_count =
                    Conversation::count_local_messages(action.local_id, guard.tether()).await?;
                if conv_count > 0 {
                    let api = ctx.api().clone();
                    let remote_msgs = ctx.spawn(async move {
                        Message::fetch_metadata(
                            GetMessagesOptions {
                                conversation_id: Some(remote_id),
                                ..Default::default()
                            },
                            &api,
                        )
                        .await
                    });
                    let mut local_msgs: HashMap<_, _> =
                        Message::in_conversation(action.local_id, guard.tether())
                            .await?
                            .into_iter()
                            .filter(|msg| !msg.is_draft() && msg.remote_id.is_some())
                            .map(|msg| (msg.remote_id.clone(), msg))
                            .collect();
                    let AsyncTaskResult::Completed(Ok(remote_msgs)) = remote_msgs
                        .await
                        .map_err(|e| anyhow!("Failed to download remote labels: `{e}`"))?
                    else {
                        return Err(MailActionError::Other(anyhow!(
                            "The task was cancelled, we need to run refresh again"
                        )));
                    };
                    guard
                        .tx(async |tx| {
                            for remote_msg in remote_msgs.messages {
                                let mut remote_msg =
                                    Message::from_api_metadata(remote_msg, tx).await?;
                                local_msgs.remove(&remote_msg.remote_id.clone());
                                remote_msg.save(tx).await?;
                            }

                            for local_msg in local_msgs.into_values() {
                                local_msg.delete(tx).await?;
                            }

                            Result::<(), <Self::Action as Action>::Error>::Ok(())
                        })
                        .await?;
                }
            }
        }

        Ok(())
    }
}
