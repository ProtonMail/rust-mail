use crate::AppError;
use crate::events::ConversationEvent;
use crate::models::Conversation;
use crate::user_context::events::subscriber::PostEventSyncData;
use proton_core_common::events::Action;
use stash::params;
use stash::stash::Bond;
use tracing::warn;

pub async fn handle_conversation_events(
    tx: &Bond<'_>,
    events: &[ConversationEvent],
    data: &mut PostEventSyncData,
) -> Result<(), AppError> {
    for event in events {
        event.action.log_entry(&event.remote_id);

        match event.action {
            Action::Delete => {
                tx.execute(
                    "DELETE FROM conversations WHERE remote_id = ?",
                    params![event.remote_id.clone()],
                )
                .await?;
            }

            Action::Create => {
                let Some(cnv) = event.conversation.clone() else {
                    warn!("Got a conversation-event without any conversation, skipping it");
                    continue;
                };

                let ids = Conversation::create_or_update_conversations(vec![cnv], tx).await?;

                data.cnv_for_prefetch.extend(ids);
            }

            Action::Update | Action::UpdateFlags => {
                let Some(cnv) = event.conversation.clone() else {
                    warn!("Got a conversation-event without any conversation, skipping it");
                    continue;
                };

                Conversation::create_or_update_conversations(vec![cnv], tx).await?;
            }
        }
    }

    Ok(())
}
