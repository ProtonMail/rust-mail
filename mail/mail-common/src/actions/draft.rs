mod save;
mod send;

use crate::cache::CacheMessageKey;
use crate::datatypes::SystemLabelId;
use crate::decrypted_message::StorableMessageBody;
use crate::models::{Label, Message};
use crate::{AppError, MailContextError, MailUserContext};
use proton_core_common::datatypes::{IdCounterpart, LabelId, LocalId};
pub use save::*;
pub use send::*;
use stash::stash::Tether;
use tracing::error;

/// Resolve the Drafts folder local label id.
async fn local_draft_label_id(tether: &Tether) -> Result<LocalId, MailContextError> {
    let Some(local_draft_label_id) = LabelId::drafts().counterpart::<Label>(tether).await? else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

/// Resolve the Sent folder  local label id.
async fn local_sent_label_id(tether: &Tether) -> Result<LocalId, MailContextError> {
    let Some(local_draft_label_id) = LabelId::sent().counterpart::<Label>(tether).await? else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

/// Load a draft message body into memory
fn load_message_body(
    context: &MailUserContext,
    message: &Message,
) -> Result<StorableMessageBody, AppError> {
    let key = CacheMessageKey::from(message);
    let Some(message_body_reader) = context.messages_cache().get_item(&key)? else {
        return Err(AppError::MessageBodyMissing(message.local_id.unwrap()));
    };

    StorableMessageBody::from_reader(message_body_reader)
        .inspect_err(|e| error!("Failed to load message body: {e}"))
}
