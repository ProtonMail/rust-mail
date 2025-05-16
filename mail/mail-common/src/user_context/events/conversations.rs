use crate::AppError;
use crate::events::ConversationEvent;
use crate::models::Conversation;
use proton_core_common::events::Action;
use proton_mail_ids::LocalConversationId;
use stash::params;
use stash::stash::Bond;
use tracing::warn;

pub async fn handle_conversation_events(
    tx: &Bond<'_>,
    conversation_events: &[ConversationEvent],
) -> Result<Vec<LocalConversationId>, AppError> {
    let mut ids = Vec::with_capacity(conversation_events.len());
    for conversation_event in conversation_events {
        conversation_event
            .action
            .log_entry(&conversation_event.remote_id);
        match conversation_event.action {
            Action::Delete => {
                tx.execute(
                    "DELETE FROM conversations WHERE remote_id = ?",
                    params![conversation_event.remote_id.clone()],
                )
                .await?;
            }
            Action::Create => {
                if let Some(conversation) = conversation_event.conversation.clone() {
                    let created =
                        Conversation::create_or_update_conversations(vec![conversation], tx)
                            .await?;
                    ids.extend(created);
                } else {
                    warn!("Received create without conversation");
                }
            }
            Action::Update | Action::UpdateFlags => {
                if let Some(conversation) = conversation_event.conversation.clone() {
                    Conversation::create_or_update_conversations(vec![conversation], tx).await?;
                } else {
                    warn!("Received update without conversation");
                }
            }
        }
    }
    Ok(ids)
}
