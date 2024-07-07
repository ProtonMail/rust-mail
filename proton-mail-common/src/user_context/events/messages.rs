use crate::events::MessageEvent;
use crate::models::Message;
use crate::AppError;
use proton_core_common::events::Action;
use stash::params;
use stash::stash::Tether;
use tracing::warn;

pub async fn handle_message_events(
    tx: &Tether,
    message_events: &[MessageEvent],
) -> Result<(), AppError> {
    for message_event in message_events {
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
                    Message::create_or_update_messages_from_metadata(
                        vec![message.clone()],
                        tx.stash(),
                    )
                    .await?;
                } else {
                    warn!("Received create message without message");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if let Some(message) = &message_event.message {
                    Message::create_or_update_messages_from_metadata(
                        vec![message.clone()],
                        tx.stash(),
                    )
                    .await?;
                } else {
                    warn!("Received update message without label");
                }
            }
        }
    }
    Ok(())
}
