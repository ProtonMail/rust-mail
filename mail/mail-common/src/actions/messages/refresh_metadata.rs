use std::collections::HashSet;

use crate::actions::MailActionError;
use crate::models::{Message, MessageScrollData};
use crate::{AppError, MailUserContext};
use itertools::Itertools;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
};
use proton_core_common::models::ModelExtension;
use proton_mail_ids::LocalMessageId;
use serde::{self, Deserialize, Serialize};
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
    local_ids: Vec<LocalMessageId>,
}

impl RefreshMetadata {
    pub fn new(local_ids: Vec<LocalMessageId>) -> Self {
        Self { local_ids }
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
        if action.local_ids.is_empty() {
            tracing::debug!("Refresh metadata for messages called with empty id list");
            return Ok(());
        }

        let messages = Message::find_by_ids(action.local_ids.clone(), guard.tether()).await?;
        let remote_ids = messages
            .iter()
            .filter(|msg| !msg.is_draft())
            .filter_map(|msg| msg.remote_id.clone())
            .collect_vec();

        let items_sync_result =
            Message::sync_metadata(remote_ids.clone(), ctx.api(), &mut guard).await;
        let refreshed_items = match items_sync_result {
            Ok(items) => items,
            Err(AppError::API(e)) if e.is_network_failure() => {
                return Err(MailActionError::Http(e));
            }
            Err(e) => {
                tracing::error!("Unexpected error while refreshing messages metadata: `{e}`");
                tracing::info!("Deleting local messages: `{:?}`", action.local_ids);
                guard
                    .tx(async |tx| {
                        Message::delete_by_ids(action.local_ids.clone(), tx).await?;
                        MessageScrollData::delete_all(tx).await?;
                        Result::<(), <Self::Action as Action>::Error>::Ok(())
                    })
                    .await?;

                return Err(e.into());
            }
        };
        let refreshed_ids: HashSet<_> = refreshed_items
            .iter()
            .filter_map(|msg| msg.local_id)
            .collect();
        let not_refreshed = action
            .local_ids
            .iter()
            .filter(|x| !refreshed_ids.contains(x))
            .copied()
            .collect_vec();

        if !not_refreshed.is_empty() {
            // The conversation appears to be not found remotely, delete it.
            tracing::warn!("Local messages without remote counterpart found while refreshing.");
            tracing::info!("Deleting local messages: `{:?}`", not_refreshed);
            guard
                .tx(async |tx| {
                    Message::delete_by_ids(not_refreshed, tx).await?;
                    MessageScrollData::delete_all(tx).await?;
                    Result::<(), <Self::Action as Action>::Error>::Ok(())
                })
                .await?;
        }

        Ok(())
    }
}
