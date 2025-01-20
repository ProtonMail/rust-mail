mod discard;
mod save;
mod send;

use crate::datatypes::SystemLabelId;
use crate::{AppError, MailContextError};
pub use discard::*;
use proton_api_core::services::proton::common::LabelId;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelIdExtension};
pub use save::*;
pub use send::*;
use stash::stash::Tether;

/// Resolve the Drafts folder local label id.
async fn local_draft_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) =
        Label::remote_id_counterpart(LabelId::drafts(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

/// Resolve the Sent folder local label id.
async fn local_sent_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) = Label::remote_id_counterpart(LabelId::sent(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

/// Resolve the Outbox folder local label id.
async fn local_outbox_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) =
        Label::remote_id_counterpart(LabelId::outbox(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}
