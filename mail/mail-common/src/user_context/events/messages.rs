use crate::models::Message;
use crate::user_context::events::event_model::MessageEvent;
use crate::{AppError, user_context::events::event_subscriber::PostEventSyncData};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::services::proton::LabelId;
use proton_core_common::event_loop::events::Action;
use proton_core_common::models::ModelIdExtension;
use stash::params;
use stash::stash::Bond;
use std::collections::HashSet;
use tracing::warn;

#[cfg(feature = "foundation_search")]
use crate::user_context::events::search::handle_search_indexing_for_message;

pub async fn handle_message_events(
    tx: &Bond<'_>,
    events: &[MessageEvent],
    rebase_change_set: &mut RebaseChangeSet,
    data: &mut PostEventSyncData,
    _unresolved_label_ids: &HashSet<LabelId>,
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
                // Handle search indexing removal before deleting the message
                #[cfg(feature = "foundation_search")]
                {
                    if let Err(e) = handle_search_indexing_for_message(
                        tx,
                        &event.remote_id,
                        event.action,
                        None, // Will look up local_id if needed
                    )
                    .await
                    {
                        warn!(
                            "Failed to handle search indexing removal for message {}: {}",
                            event.remote_id, e
                        );
                    }
                }

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

                // Handle search indexing for newly created messages
                #[cfg(feature = "foundation_search")]
                {
                    if let Some(local_id) = ids.first()
                        && let Err(e) = handle_search_indexing_for_message(
                            tx,
                            &event.remote_id,
                            event.action,
                            Some(local_id.as_u64()),
                        )
                        .await
                    {
                        warn!(
                            "Failed to handle search indexing for message {}: {}",
                            event.remote_id, e
                        );
                    }
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

                // Handle search indexing for updated messages
                #[cfg(feature = "foundation_search")]
                {
                    if let Some(local_id) = ids.first()
                        && let Err(e) = handle_search_indexing_for_message(
                            tx,
                            &event.remote_id,
                            event.action,
                            Some(local_id.as_u64()),
                        )
                        .await
                    {
                        warn!(
                            "Failed to handle search indexing for message {}: {}",
                            event.remote_id, e
                        );
                    }
                }

                rebase_change_set.add_many(ids);
            }
        }
    }

    Ok(())
}
