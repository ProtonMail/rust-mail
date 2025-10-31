use crate::AppError;
use crate::actions::MailActionError;
use crate::datatypes::{ConversationViewOptions, LocalConversationId};
use crate::models::{Conversation, ConversationScrollData};
use crate::{MailUserContext, models::Message};
use anyhow::anyhow;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::prelude::GetMessagesOptions;
use serde::{self, Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use std::collections::{HashMap, HashSet};
use std::sync::Weak;

/// Refresh conversation metadata action.
///
/// This action is designed to refresh existing metadata
/// in a case of suspicion that this conversation may be outdated.
///
/// On failure the local conversation will be removed.
///
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RefreshMetadata {
    local_ids: Vec<LocalConversationId>,
}

impl RefreshMetadata {
    pub fn new(local_ids: Vec<LocalConversationId>) -> Self {
        Self { local_ids }
    }
}

impl Action for RefreshMetadata {
    const TYPE: Type = Type("refresh_conversation_metadata");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Normal;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = RefreshMetadataHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct RefreshMetadataHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler for RefreshMetadataHandler {
    type Action = RefreshMetadata;

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
        let ctx = self.ctx.upgrade().ok_or(MailActionError::LostContext)?;

        if action.local_ids.is_empty() {
            tracing::debug!("Refresh metadata for conversations called with empty id list");
            return Ok(());
        }

        let remote_ids =
            Conversation::local_ids_counterpart(action.local_ids.clone(), guard.tether()).await?;

        if remote_ids.is_empty() {
            tracing::debug!(
                "All the conversations ({}) misses their remote_id, skip",
                action.local_ids.len()
            );
            return Ok(());
        }

        let items_sync_result =
            Conversation::sync_metadata(remote_ids.clone(), ctx.session(), &mut guard).await;

        let refreshed_items = match items_sync_result {
            Ok(items) => items,
            Err(AppError::API(e)) if e.is_network_failure() => {
                return Err(MailActionError::Http(e));
            }
            Err(e) => {
                tracing::error!("Unexpected error while refreshing conversations metadata: `{e}`");
                tracing::info!("Deleting local conversations: `{:?}`", action.local_ids);
                guard
                    .tx(async |tx| {
                        Conversation::delete_by_ids(action.local_ids.clone(), tx).await?;
                        ConversationScrollData::delete_all(tx).await?;
                        Result::<(), <Self::Action as Action>::Error>::Ok(())
                    })
                    .await?;

                return Err(e.into());
            }
        };

        let refreshed_ids: HashSet<_> = refreshed_items
            .iter()
            .filter_map(|conv| conv.local_id)
            .collect();

        let mut not_refreshed = Vec::new();

        for not_fresh in action
            .local_ids
            .iter()
            .filter(|x| !refreshed_ids.contains(x))
            .copied()
        {
            if Conversation::local_id_counterpart(not_fresh, guard.tether())
                .await?
                .is_some()
            {
                not_refreshed.push(not_fresh);
            }
        }

        if !not_refreshed.is_empty() {
            // The conversation appears to be not found remotely, delete it.
            tracing::warn!("Local conversation without remote counterpart found while refreshing.");
            tracing::info!("Deleting local conversations: `{:?}`", not_refreshed);

            guard
                .tx(async |tx| {
                    Conversation::delete_by_ids(not_refreshed, tx).await?;
                    ConversationScrollData::delete_all(tx).await?;
                    Result::<(), <Self::Action as Action>::Error>::Ok(())
                })
                .await?;
        }

        for conv in refreshed_items {
            refresh_conversation_messages(conv, &ctx, &mut guard).await?;
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

async fn refresh_conversation_messages(
    conversation: Conversation,
    ctx: &MailUserContext,
    guard: &mut WriterGuard<'_>,
) -> Result<(), MailActionError> {
    let local_id = conversation.id();
    let conv_count = Conversation::message_count(local_id, guard.tether()).await?;
    if conv_count > 0 {
        let api = ctx.session().clone();
        let remote_msgs = ctx.spawn(async move {
            Message::fetch_metadata(
                GetMessagesOptions {
                    conversation_id: Some(conversation.remote_id.clone().unwrap()),
                    ..Default::default()
                },
                &api,
            )
            .await
        });
        let mut local_msgs: HashMap<_, _> =
            Message::in_conversation(local_id, ConversationViewOptions::All, guard.tether())
                .await?
                .into_iter()
                .filter(|msg| msg.remote_id.is_some())
                .map(|msg| (msg.remote_id.clone(), msg))
                .collect();

        let remote_msgs = match remote_msgs.await {
            Ok(msgs) => msgs.map_err(|e| anyhow!("Failed to download remote labels: `{e}`"))?,
            Err(_) => {
                return Err(MailActionError::Other(anyhow!(
                    "The task was cancelled, we need to run refresh again"
                )));
            }
        };

        guard
            .tx(async |tx| {
                for remote_msg in remote_msgs.messages {
                    let mut remote_msg = Message::from_api_metadata(remote_msg, tx).await?;
                    let local_msg = local_msgs.remove(&remote_msg.remote_id.clone());
                    match local_msg {
                        Some(local_msg) => {
                            if !local_msg.is_local_draft(tx).await? {
                                remote_msg.save(tx).await?;
                            }
                        }
                        None => remote_msg.save(tx).await?,
                    }
                }

                // `local_msgs` map is filtered by remote_id
                // so even if it is a draft it had to be removed from remote
                // remove it locally as well
                for local_msg in local_msgs.into_values() {
                    if !local_msg.is_local_draft(tx).await? {
                        local_msg.delete(tx).await?;
                    }
                }

                Result::<(), MailActionError>::Ok(())
            })
            .await?;
    }

    Ok(())
}
