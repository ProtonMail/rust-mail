use crate::MailContextError;
use crate::actions::draft::{SEND_ACTION_GROUP, save_attachment_error};
use crate::datatypes::attachment::CombinedAttachmentDisposition;
use crate::datatypes::{Disposition, LocalAttachmentId};
use crate::draft::AttachmentDispositionSwapError;
use crate::models::{
    Attachment, DraftAttachmentMetadata, DraftAttachmentUploadState, DraftMetadata,
    DraftSendResultOrigin, MetadataId,
};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::consts::{General, Mail};
use proton_core_api::service::ApiServiceError;
use proton_core_api::session::Session;
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::request_data::NewAttachmentDisposition;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;

#[derive(Serialize, Deserialize)]
pub struct AttachmentDispositionUpdate {
    metadata_id: MetadataId,
    attachment_id: LocalAttachmentId,
    mode: DraftAttachmentDispositionUpdateMode,
}
impl AttachmentDispositionUpdate {
    pub fn new(
        metadata_id: MetadataId,
        attachment_id: LocalAttachmentId,
        new_disposition: CombinedAttachmentDisposition,
    ) -> Self {
        Self {
            metadata_id,
            attachment_id,
            mode: DraftAttachmentDispositionUpdateMode::Swap(new_disposition),
        }
    }

    pub fn retry(metadata_id: MetadataId, attachment_id: LocalAttachmentId) -> Self {
        Self {
            metadata_id,
            attachment_id,
            mode: DraftAttachmentDispositionUpdateMode::Retry,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DraftAttachmentDispositionUpdateMode {
    Swap(CombinedAttachmentDisposition),
    Retry,
}

impl Action for AttachmentDispositionUpdate {
    const TYPE: Type = Type("draft_attachment_disposition_update");
    const GROUP: ActionGroup = SEND_ACTION_GROUP;
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::High;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = AttachmentDispositionUpdateHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct AttachmentDispositionUpdateHandler {
    pub api: Session,
}

impl Handler for AttachmentDispositionUpdateHandler {
    type Action = AttachmentDispositionUpdate;

    async fn apply_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let mut metadata = DraftAttachmentMetadata::find_by_id(action.attachment_id, tx)
            .await?
            .ok_or(AttachmentDispositionSwapError::AttachmentMetadataNotFound(
                action.attachment_id,
            ))?;
        match &action.mode {
            DraftAttachmentDispositionUpdateMode::Swap(disposition) => {
                tracing::info!(
                    "Changing disposition for {:?} to {:?}",
                    action.attachment_id,
                    disposition
                );
            }
            DraftAttachmentDispositionUpdateMode::Retry => {
                tracing::info!("Retrying disposition change for {:?}", action.attachment_id,);
                if !(metadata.state() == DraftAttachmentUploadState::Uploaded
                    || metadata.is_disposition_swap_error())
                {
                    tracing::error!("Attachment is in invalid state {:?}", metadata.state());
                    return Err(
                        AttachmentDispositionSwapError::InvalidState(action.attachment_id).into(),
                    );
                }
            }
        }

        // we only want to do this if we have not had an error
        if let DraftAttachmentDispositionUpdateMode::Swap(disposition) = &action.mode {
            let mut attachment = Attachment::find_by_id(action.attachment_id, tx)
                .await?
                .ok_or(AttachmentDispositionSwapError::AttachmentNotFound(
                    action.attachment_id,
                ))?;

            match attachment.disposition {
                Disposition::Attachment => {
                    if let CombinedAttachmentDisposition::Inline(cid) = disposition {
                        attachment.disposition = Disposition::Inline;
                        attachment.content_id = Some(cid.clone())
                    } else {
                        return Err(AttachmentDispositionSwapError::Noop.into());
                    }
                }
                Disposition::Inline
                    if matches!(disposition, CombinedAttachmentDisposition::Attachment) =>
                {
                    attachment.disposition = Disposition::Attachment;
                }
                _ => {
                    return Err(AttachmentDispositionSwapError::Noop.into());
                }
            };
            attachment.save(tx).await?;
        }

        // reset to uploaded state
        metadata.set_disposition_swap_state();
        metadata.action_id = Some(this_id);
        metadata.save(tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to undo, we remain in an error state
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut writer_guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let message_id = DraftMetadata::find_by_id(action.metadata_id, writer_guard.tether())
            .await?
            .ok_or(AttachmentDispositionSwapError::MetadataNotFound(
                action.metadata_id,
            ))?
            .local_message_id
            .ok_or(AttachmentDispositionSwapError::NoMessageIdInDraftMetadata(
                action.metadata_id,
            ))?;

        let r = async {
            let mut metadata =
                DraftAttachmentMetadata::find_by_id(action.attachment_id, writer_guard.tether())
                    .await?
                    .ok_or(AttachmentDispositionSwapError::AttachmentMetadataNotFound(
                        action.attachment_id,
                    ))?;

            let mut attachment =
                Attachment::find_by_id(action.attachment_id, writer_guard.tether())
                    .await?
                    .ok_or(AttachmentDispositionSwapError::AttachmentNotFound(
                        action.attachment_id,
                    ))?;
            let Some(remote_id) = attachment.remote_id() else {
                return Err(AttachmentDispositionSwapError::AttachmentHasNoRemoteId(
                    action.attachment_id,
                )
                .into());
            };

            // reset state to disp. swap again. If the user adds an inline attachment and swaps
            // the disposition in quick succession, the state after upload will be Uploaded and
            // some events will not fire.
            metadata.set_disposition_swap_state();
            writer_guard
                .tx(async |tx| {
                    // we need to reset the attachment state here since it's possible that after attachment upload
                    // the state will be rest after the attachment is uploaded to the server. If we don't
                    // the client UI will not update correctly.
                    if let DraftAttachmentDispositionUpdateMode::Swap(disp) = &action.mode {
                        match disp {
                            CombinedAttachmentDisposition::Attachment => {
                                attachment.disposition = Disposition::Attachment;
                            }
                            CombinedAttachmentDisposition::Inline(cid) => {
                                attachment.disposition = Disposition::Inline;
                                attachment.content_id = Some(cid.clone());
                            }
                        }
                        attachment.save(tx).await?;
                    }
                    Ok(metadata.save(tx).await?)
                })
                .await
                .inspect_err(|e: &MailContextError| {
                    tracing::error!("Failed to update attachment metadata before request: {e}")
                })?;

            let new_disposition = match action.mode.clone() {
                DraftAttachmentDispositionUpdateMode::Retry => {
                    let attachment =
                        Attachment::find_by_id(action.attachment_id, writer_guard.tether())
                            .await?
                            .ok_or(AttachmentDispositionSwapError::AttachmentNotFound(
                                action.attachment_id,
                            ))?;
                    match attachment.disposition {
                        Disposition::Attachment => NewAttachmentDisposition::Attachment,
                        Disposition::Inline => {
                            let cid = attachment.content_id.ok_or(
                                AttachmentDispositionSwapError::AttachmentHasNoContentId(
                                    action.attachment_id,
                                ),
                            )?;
                            NewAttachmentDisposition::Inline(cid.into_inner())
                        }
                    }
                }
                DraftAttachmentDispositionUpdateMode::Swap(disposition) => disposition.into(),
            };

            tracing::info!(
                "Changing disposition for {:?} to {:?}",
                remote_id,
                new_disposition
            );

            if let Err(e) = self
                .api
                .put_attachment_disposition(remote_id.clone(), new_disposition)
                .await
            {
                tracing::error!("Failed to swap disposition: {e}");
                let ApiServiceError::UnprocessableEntity(v, Some(error)) = e else {
                    return Err(MailContextError::from(e));
                };
                Err(if error.code == Mail::AttachmentDoesNotExist as u32 {
                    AttachmentDispositionSwapError::AttachmentDoesNotExistServer(remote_id).into()
                } else if error.code == Mail::AttachmentMessageNotADraft as u32 {
                    AttachmentDispositionSwapError::AttachmentMessageIsNotADraft(remote_id).into()
                } else if error.code == Mail::AttachmentMessageDoesNotExist as u32 {
                    AttachmentDispositionSwapError::AttachmentMessageDoesNotExist(remote_id).into()
                } else if error.code == General::InvalidRequirements as u32 {
                    AttachmentDispositionSwapError::AttachmentDoesNotHaveValidCid(remote_id).into()
                } else {
                    ApiServiceError::UnprocessableEntity(v, Some(error)).into()
                })
            } else {
                metadata.set_uploaded_state();
                writer_guard
                    .tx(async |tx| Ok(metadata.save(tx).await?))
                    .await
                    .inspect_err(|e: &MailContextError| {
                        tracing::error!("Failed to update attachment metadata: {e}")
                    })?;
                Ok(())
            }
        }
        .await;

        if let Err(err) = &r {
            if let Err(e) = save_attachment_error(
                message_id,
                action.attachment_id,
                DraftSendResultOrigin::AttachmentDispositionSwap,
                &mut writer_guard,
                err,
            )
            .await
            {
                tracing::error!("Failed to update attachment metadata state: {e}");
            }
        }

        r
    }
    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &RebaseChangeSet,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
