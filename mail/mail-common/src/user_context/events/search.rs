//! Search indexing event handler
//!
//! This module handles search indexing for message events, following the subscriber pattern.
//! It provides functions that can be called from event handlers to queue search indexing
//! operations within the same transaction for atomicity.

use crate::AppError;
use crate::models::Message;
use proton_core_common::event_loop::events::Action;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::common::MessageId;
use stash::stash::Bond;
use tracing::warn;

#[cfg(feature = "foundation_search")]
use crate::search::MailSearchService;

/// Handle search indexing for a single message event (v6 event structure)
///
/// This function handles search indexing for a single message event, used by the v6 event subscriber.
/// It takes the message ID, action, and local message ID (if available).
///
/// # Arguments
///
/// * `tx` - Database transaction bond
/// * `remote_id` - Remote message ID
/// * `action` - Action type (Create, Update, Delete, etc.)
/// * `local_id` - Optional local message ID (if message was created/updated)
///
/// # Returns
///
/// Returns `Ok(())` on success, or an `AppError` if indexing operations fail.
#[cfg(feature = "foundation_search")]
pub async fn handle_search_indexing_for_message(
    tx: &Bond<'_>,
    remote_id: &MessageId,
    action: Action,
    local_id: Option<u64>,
) -> Result<(), AppError> {
    match action {
        Action::Delete => {
            // If we have a local_id, use it directly. Otherwise, look it up.
            let local_id = if let Some(id) = local_id {
                id
            } else if let Ok(Some(msg)) =
                Message::remote_id_counterpart(remote_id.clone(), tx).await
            {
                msg.as_u64()
            } else {
                return Ok(()); // Message not found, nothing to remove
            };

            // Queue search removal
            if let Err(e) = MailSearchService::queue_remove(local_id, tx).await {
                warn!(
                    "Failed to queue search removal for message {}: {}",
                    local_id, e
                );
            }
        }

        Action::Create | Action::Update | Action::UpdateFlags => {
            // If we have a local_id, use it directly. Otherwise, look it up.
            let local_id = if let Some(id) = local_id {
                id
            } else if let Ok(Some(msg)) =
                Message::remote_id_counterpart(remote_id.clone(), tx).await
            {
                msg.as_u64()
            } else {
                return Ok(()); // Message not found, nothing to index
            };

            if let Err(e) = MailSearchService::queue_index(local_id, tx).await {
                warn!(
                    "Failed to queue search indexing for message {}: {}",
                    local_id, e
                );
            }
        }
    }

    Ok(())
}

/// No-op implementation when foundation_search feature is disabled
#[cfg(not(feature = "foundation_search"))]
pub async fn handle_search_indexing_for_message(
    _tx: &Bond<'_>,
    _remote_id: &MessageId,
    _action: Action,
    _local_id: Option<u64>,
) -> Result<(), AppError> {
    Ok(())
}
