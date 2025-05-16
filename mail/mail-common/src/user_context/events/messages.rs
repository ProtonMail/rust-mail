use crate::AppError;
use crate::events::MessageEvent;
use crate::models::{DraftMetadata, Message};
use proton_core_common::events::Action;
use proton_mail_ids::LocalMessageId;
use stash::params;
use stash::stash::Bond;
use tracing::{debug, warn};

pub async fn handle_message_events(
    tx: &Bond<'_>,
    message_events: &[MessageEvent],
) -> Result<Vec<LocalMessageId>, AppError> {
    let mut ids = Vec::with_capacity(message_events.len());
    for message_event in message_events {
        message_event.action.log_entry(&message_event.remote_id);
        match message_event.action {
            Action::Delete => {
                tx.execute(
                    "DELETE FROM messages WHERE remote_id = ?",
                    params![message_event.remote_id.clone()],
                )
                .await?;
            }
            Action::Create => {
                if let Some(message) = &message_event.message {
                    let created =
                        Message::create_or_update_messages_from_metadata(vec![message.clone()], tx)
                            .await?;
                    ids.extend(created);
                } else {
                    warn!("Received create message without message");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if DraftMetadata::exists_for_message_with_remote_id(
                    message_event.remote_id.clone(),
                    tx,
                )
                .await?
                {
                    debug!(
                        "Skipping message update for {} due to draft metadata",
                        message_event.remote_id
                    );
                    continue;
                }
                if let Some(message) = &message_event.message {
                    Message::create_or_update_messages_from_metadata(vec![message.clone()], tx)
                        .await?;
                } else {
                    warn!("Received update message without label");
                }
            }
        }
    }

    Ok(ids)
}
