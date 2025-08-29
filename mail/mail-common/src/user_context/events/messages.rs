use crate::datatypes::MessageFlags;
use crate::events::MessageEvent;
use crate::models::{DraftMetadata, Message, MessageBody};
use crate::{AppError, user_context::events::subscriber::PostEventSyncData};
use proton_core_common::events::Action;
use proton_core_common::models::ModelIdExtension;
use stash::params;
use stash::stash::Bond;
use tracing::warn;

pub async fn handle_message_events(
    tx: &Bond<'_>,
    events: &[MessageEvent],
    data: &mut PostEventSyncData,
) -> Result<(), AppError> {
    for event in events {
        event.action.log_entry(&event.remote_id);

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

                let ids =
                    Message::create_or_update_messages_from_metadata(vec![message.clone()], tx)
                        .await?;

                data.msg_for_prefetch.extend(ids);
            }

            Action::Update | Action::UpdateFlags => {
                let Some(message) = &event.message else {
                    warn!("Got a message-event without any message, skipping it");
                    continue;
                };

                // Here the following cases can happen:
                // 1. It's a draft, we don't have it open: Treat it as a normal message, update.
                // 2. It's a draft, we have it open: Skip body and metadata updates because we
                // don't have conflict resolution strategies in place
                // 3. It _was_ a draft, we have it open, now it has been sent: We might have
                // missed updates, let's do a full update.
                // 4. If it's `Action::Update` we need to update the body (except of course if the
                //    draft is open)

                let mut is_stale_draft = false;
                if DraftMetadata::find_by_message_with_remote_id(message.id.clone(), tx)
                    .await?
                    .is_some()
                {
                    // We have a message that has been opened as a draft, but it is possible that
                    // another session has sent this draft. Deleting the metadata at this point in
                    // time can trigger the composer to display a collection of metadata not found errors
                    // that can be very confusing for the user.
                    // We let the update progress and the next action that executes for that
                    // draft will trigger a failure and clean itself up.
                    // It's possible that some messages will never properly clean up this way, but
                    // this should happen very often and the associated metadata is not very large
                    // with each draft. Correctly solving this requires knowledge of active composer
                    // states on the rust side.

                    let flags = MessageFlags::from(message.flags);
                    if !(flags.is_schedule_send() || flags.is_sent()) {
                        // Case 2.
                        tracing::info!(
                            "Skipping message update for {} because it's opened locally",
                            message.id
                        );
                        continue;
                    }

                    // Case 3.
                    // We delete the local message body so that it gets re-requested
                    // whenever it gets open again. This is because we're skipping updates.
                    // Since we're skipping previous `Action::Update`s, this could be just an
                    // `Action::UpdateFlags` and we would have a stale body.
                    tracing::debug!(
                        "Message {} has draft metadata but was already sent, update will be allowed",
                        message.id
                    );

                    is_stale_draft = true;
                }

                // Case 4.
                if (event.action == Action::Update || is_stale_draft)
                    && let Some(local_id) =
                        Message::remote_id_counterpart(message.id.clone(), tx).await?
                {
                    _ = MessageBody::delete(local_id, tx).await;
                }

                Message::create_or_update_messages_from_metadata(vec![message.clone()], tx).await?;
            }
        }
    }

    Ok(())
}
