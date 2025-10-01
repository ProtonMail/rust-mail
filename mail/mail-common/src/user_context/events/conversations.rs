use crate::AppError;
use crate::events::ConversationEvent;
use crate::models::Conversation;
use crate::user_context::events::subscriber::PostEventSyncData;
use proton_core_common::events::Action;
use proton_core_common::models::ModelIdExtension;
use stash::params;
use stash::stash::Bond;
use tracing::warn;

pub async fn handle_conversation_events(
    tx: &Bond<'_>,
    events: &[ConversationEvent],
    data: &mut PostEventSyncData,
) -> Result<(), AppError> {
    for event in events {
        event
            .action
            .log_entry(&event.remote_id, async |remote_id| {
                Conversation::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;

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
                if !ids.is_empty() {
                    tracing::info!("Created with {:?}", ids[0]);
                }
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
