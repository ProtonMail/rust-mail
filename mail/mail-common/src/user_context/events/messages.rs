use crate::events::MessageEvent;
use crate::models::Message;
use crate::{AppError, user_context::events::subscriber::PostEventSyncData};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_common::events::Action;
use proton_core_common::models::ModelIdExtension;
use stash::params;
use stash::stash::Bond;
use tracing::warn;

pub async fn handle_message_events(
    tx: &Bond<'_>,
    events: &[MessageEvent],
    rebase_change_set: &mut RebaseChangeSet,
    data: &mut PostEventSyncData,
) -> Result<(), AppError> {
    for event in events {
        event
            .action
            .log_entry(&event.remote_id, async |remote_id| {
                Message::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;

        match event.action {
            Action::Delete => {
                tx.execute(
                    "DELETE FROM messages WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await?;
            }

            Action::Create => {
                let Some(message) = &event.message else {
                    warn!("Got a message-event without any message, skipping it");
                    continue;
                };

                let ids = Message::create_or_update_messages_from_metadata(
                    vec![message.clone()],
                    Some(event.action),
                    tx,
                )
                .await?;

                if !ids.is_empty() {
                    tracing::info!("Created with {:?}", ids[0]);
                }

                data.msg_for_prefetch.extend(ids.iter().copied());
                rebase_change_set.add_many(ids);
            }

            Action::Update | Action::UpdateFlags => {
                let Some(message) = &event.message else {
                    warn!("Got a message-event without any message, skipping it");
                    continue;
                };

                let ids = Message::create_or_update_messages_from_metadata(
                    vec![message.clone()],
                    Some(event.action),
                    tx,
                )
                .await?;
                rebase_change_set.add_many(ids);
            }
        }
    }

    Ok(())
}
