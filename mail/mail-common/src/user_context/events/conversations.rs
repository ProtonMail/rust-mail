use crate::AppError;
use crate::models::Conversation;
use crate::user_context::events::event_model::ConversationEvent;
use crate::user_context::events::event_subscriber::PostEventSyncData;
use proton_action_queue::rebase::RebaseChangeSet;
use stash::stash::Bond;

pub async fn handle_conversation_events(
    tx: &Bond<'_>,
    events: &mut [ConversationEvent],
    changeset: &mut RebaseChangeSet,
    data: &mut PostEventSyncData,
) -> Result<(), AppError> {
    for event in events {
        if let Some(id) = Conversation::handle_event(
            tx,
            &event.remote_id,
            event.action,
            event.conversation.as_mut(),
            changeset,
        )
        .await?
        {
            data.cnv_for_prefetch.push(id)
        }
    }

    Ok(())
}
