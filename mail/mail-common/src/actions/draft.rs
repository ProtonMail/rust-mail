mod attachment_disposition_update;
mod attachment_remove;
mod attachment_upload;
mod discard;
mod save;
mod send;
mod undo_send;

pub use self::attachment_disposition_update::*;
pub use self::attachment_remove::*;
pub use self::attachment_upload::*;
pub use self::discard::*;
pub use self::save::*;
pub use self::send::*;
pub use self::undo_send::*;
use crate::datatypes::{LocalAttachmentId, LocalMessageId, SystemLabelId};
use crate::models::{
    DraftAttachmentInternalError, DraftAttachmentMetadata, DraftSendFailure, DraftSendResult,
    DraftSendResultOrigin,
};
use crate::{AppError, MailContextError};
use proton_action_queue::action::{ActionGroup, WriterGuard, WriterGuardError};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use stash::UserDb;
use stash::orm::Model;
use stash::stash::Tether;
use tracing::error;

pub const SEND_ACTION_GROUP: ActionGroup = ActionGroup::new("MAIL_SEND");
pub const SHARE_EXT_ACTION_GROUP: ActionGroup = ActionGroup::new("SHARE_EXT");

async fn local_draft_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) =
        Label::remote_id_counterpart(LabelId::drafts(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}

async fn local_all_draft_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_all_draft_label_id) =
        Label::remote_id_counterpart(LabelId::all_drafts(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::all_drafts()).into());
    };

    Ok(local_all_draft_label_id)
}

async fn local_all_mail_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_all_mail_label_id) =
        Label::remote_id_counterpart(LabelId::all_mail(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::all_mail()).into());
    };

    Ok(local_all_mail_label_id)
}

async fn local_sent_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) = Label::remote_id_counterpart(LabelId::sent(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::sent()).into());
    };

    Ok(local_draft_label_id)
}

async fn local_all_sent_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_all_sent_label_id) =
        Label::remote_id_counterpart(LabelId::all_sent(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::all_sent()).into());
    };

    Ok(local_all_sent_label_id)
}

async fn local_outbox_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) =
        Label::remote_id_counterpart(LabelId::outbox(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::outbox()).into());
    };

    Ok(local_draft_label_id)
}

async fn local_all_scheduled_label_id(tether: &Tether) -> Result<LocalLabelId, MailContextError> {
    let Some(local_draft_label_id) =
        Label::remote_id_counterpart(LabelId::all_scheduled(), tether).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::all_scheduled()).into());
    };

    Ok(local_draft_label_id)
}

async fn save_attachment_error(
    message_id: LocalMessageId,
    attachment_id: LocalAttachmentId,
    origin: DraftSendResultOrigin,
    writer_guard: &mut WriterGuard<'_, UserDb>,
    error: &MailContextError,
) -> Result<(), WriterGuardError> {
    writer_guard
        .tx(async |tx| {
            let mut send_result = DraftSendResult::failure(
                message_id,
                origin,
                DraftSendFailure::from_mail_context_error(error),
            );

            send_result
                .save(tx)
                .await
                .inspect_err(|e| error!("Failed to save send result: {e:?}"))?;

            if let Some(mut attachment_metadata) =
                DraftAttachmentMetadata::find_by_id(attachment_id, tx).await?
            {
                if error.is_network_failure() {
                    attachment_metadata.set_offline_state();
                } else {
                    attachment_metadata.set_error_state(
                        DraftAttachmentInternalError::from_mail_context_error(origin, error),
                    );
                }
                attachment_metadata
                    .save(tx)
                    .await
                    .inspect_err(|e| error!("Failed to save draft attachment metadata: {e:?}"))?;
            }

            Ok(())
        })
        .await
}
