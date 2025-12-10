use crate::models::Message;
use crate::user_context::events::event_model::MessageEvent;
use crate::{AppError, user_context::events::event_subscriber::PostEventSyncData};
use proton_action_queue::rebase::RebaseChangeSet;
use stash::stash::Bond;

pub async fn handle_message_events(
    tx: &Bond<'_>,
    events: &[MessageEvent],
    rebase_change_set: &mut RebaseChangeSet,
    data: &mut PostEventSyncData,
) -> Result<(), AppError> {
    for event in events {
        if let Some(id) = Message::handle_event(
            tx,
            &event.remote_id,
            event.action,
            event.message.as_ref(),
            rebase_change_set,
        )
        .await?
        {
            data.msg_for_prefetch.push(id);
        }
    }

    Ok(())
}
