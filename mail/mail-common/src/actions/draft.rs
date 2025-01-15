mod save;
mod send;

use crate::cache::CacheMessageKey;
use crate::datatypes::SystemLabelId;
use crate::decrypted_message::StorableMessageBody;
use crate::models::Message;
use crate::{AppError, MailContextError, MailUserContext};
use proton_api_core::services::proton::common::LabelId;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelIdExtension};
pub use save::*;
pub use send::*;
use stash::stash::Tether;
use tracing::error;

/// Resolve the Drafts folder local label id.
async fn local_draft_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) =
        Label::remote_id_counterpart(LabelId::drafts(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

/// Resolve the Sent folder  local label id.
async fn local_sent_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) = Label::remote_id_counterpart(LabelId::sent(), tether).await?
    else {
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
