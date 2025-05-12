mod attachment_remove;
mod attachment_upload;
mod discard;
mod save;
mod send;
mod undo_send;

use crate::datatypes::SystemLabelId;
use crate::{AppError, MailContextError};
pub use attachment_remove::*;
pub use attachment_upload::*;
pub use discard::*;
use proton_action_queue::action::ActionGroup;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelIdExtension};
pub use save::*;
pub use send::*;
use stash::stash::Tether;
pub use undo_send::*;

pub const SEND_ACTION_GROUP: ActionGroup = ActionGroup::new("MAIL_SEND");

/// Resolve the Drafts folder local label id.
async fn local_draft_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) =
        Label::remote_id_counterpart(LabelId::drafts(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

/// Resolve the AllDrafts folder local label id.
async fn local_all_draft_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_all_draft_label_id) =
        Label::remote_id_counterpart(LabelId::all_drafts(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::all_drafts()).into());
    };

    Ok(local_all_draft_label_id)
}

/// Resolve the AllMail folder local label id.
async fn local_all_mail_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_all_mail_label_id) =
        Label::remote_id_counterpart(LabelId::all_mail(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::all_mail()).into());
    };

    Ok(local_all_mail_label_id)
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
