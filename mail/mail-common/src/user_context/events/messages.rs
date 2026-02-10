use crate::models::Message;
use crate::user_context::events::event_model::MessageEvent;
use crate::{AppError, user_context::events::event_subscriber::PostEventSyncData};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::services::proton::LabelId;
use stash::stash::Bond;
use std::collections::HashSet;

pub async fn handle_message_events(
    tx: &Bond<'_>,
    events: &[MessageEvent],
    rebase_change_set: &mut RebaseChangeSet,
    data: &mut PostEventSyncData,
    unresolved_label_ids: &HashSet<LabelId>,
) -> Result<(), AppError> {
    for event in events {
        if let Some(id) = Message::handle_event(
            tx,
            &event.remote_id,
            event.action,
            event.message.as_ref(),
            rebase_change_set,
            unresolved_label_ids,
        )
        .await?
        {
            data.msg_for_prefetch.push(id);
        }
    }

    Ok(())
}
