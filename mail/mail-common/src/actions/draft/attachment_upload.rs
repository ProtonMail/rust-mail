use crate::actions::draft::SEND_ACTION_GROUP;
use crate::datatypes::Disposition;
use crate::datatypes::{LocalAttachmentId, LocalMessageId};
use crate::draft::AttachmentUploadError;
use crate::models::{
    Attachment, AttachmentType, DraftAttachmentMetadata, DraftAttachmentUploadError,
    DraftAttachmentUploadState, DraftMetadata, DraftSendFailure, DraftSendResult,
    DraftSendResultOrigin, Message, MetadataId,
};
use crate::{MailContextError, MailUserContext};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
    WriterGuardError,
};
use proton_core_api::consts::Mail;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::AddressId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::{NewAttachmentDisposition, NewAttachmentParams};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Tether};
use std::sync::Weak;
use tracing::{debug, error, info, warn};

/// Action to upload attachments for a given draft.
///
/// Note that the draft must exist on the server or we can't upload attachments.
#[derive(Serialize, Deserialize)]
pub struct AttachmentUpload {
    metadata_id: MetadataId,
    address_id: AddressId,
    attachment_id: LocalAttachmentId,
    local_message_id: Option<LocalMessageId>,
    mode: AttachmentUploadMode,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Eq, PartialEq)]
pub enum AttachmentUploadMode {
    /// First time we are trying to create the attachment.
    Create,
    /// Retrying the request because something failed previously.
    Retry,
}

impl AttachmentUpload {
    pub fn new(
        metadata_id: MetadataId,
        address_id: AddressId,
        attachment_id: LocalAttachmentId,
        mode: AttachmentUploadMode,
    ) -> Self {
        Self {
            metadata_id,
            address_id,
            attachment_id,
            local_message_id: None,
            mode,
        }
    }

    async fn local_message_id(&self, tether: &Tether) -> Result<LocalMessageId, MailContextError> {
        let Some(metadata) = DraftMetadata::find_by_id(self.metadata_id, tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", self.metadata_id);
            return Err(AttachmentUploadError::MetadataNotFound(self.metadata_id).into());
        };

        let Some(message_id) = metadata.local_message_id else {
            return Err(AttachmentUploadError::MessageDoesNotExist.into());
        };

        Ok(message_id)
    }
}

impl Action for AttachmentUpload {
    const TYPE: Type = Type("attachment_upload");
    const GROUP: ActionGroup = SEND_ACTION_GROUP;
    const VERSION: u32 = 0;
    const PRIORITY: Priority = Priority::High;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = AttachmentUploadHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct AttachmentUploadHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler for AttachmentUploadHandler {
    type Action = AttachmentUpload;

    async fn apply_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let mut attachment_upload_metadata = if let Some(metadata) =
            DraftAttachmentMetadata::find_by_id(action.attachment_id, tx)
                .await
                .inspect_err(|e| error!("Failed to load draft attachment metadata: {e:?}"))?
        {
            metadata
        } else {
            let next_display_order =
                DraftAttachmentMetadata::next_display_order(action.metadata_id, tx)
                    .await
                    .inspect_err(|e| error!("Failed to get the next display order: {e:?}"))?;
            DraftAttachmentMetadata::new(
                action.metadata_id,
                action.attachment_id,
                next_display_order,
                false,
            )
        };

        tracing::info!(
            "Uploading attachment {} from draft {}",
            attachment_upload_metadata.local_attachment_id,
            attachment_upload_metadata.metadata_id
        );

        if let Some(id) = attachment_upload_metadata.action_id {
            error!(
                "Attempting to create new attachment upload action when existing action ({id}) exists"
            );
            return Err(AttachmentUploadError::ExistingUploadActionExist(id).into());
        }

        if matches!(
            attachment_upload_metadata.state(),
            DraftAttachmentUploadState::Uploaded
        ) {
            error!("This attachment has already been uploaded");
            return Err(
                AttachmentUploadError::AttachmentAlreadyUploaded(action.attachment_id).into(),
            );
        }

        attachment_upload_metadata.action_id = Some(this_id);
        attachment_upload_metadata.set_uploading_state();

        attachment_upload_metadata
            .save(tx)
            .await
            .inspect_err(|e| error!("Failed to save draft attachment metadata: {e:?}"))?;

        let message_id = action.local_message_id(tx).await?;

        // assign attachment to message.
        tx.execute(
            "INSERT OR IGNORE INTO message_attachments (local_message_id, local_attachment_id) VALUES(?,?)",
            params![message_id, action.attachment_id],
        )
            .await.inspect_err(|e| error!("Failed to assign attachment to message: {e}"))?;

        action.local_message_id = Some(message_id);
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if let Some(message_id) = action.local_message_id {
            // remove attachment from message.
            tx.execute(
                    "DELETE FROM message_attachments WHERE local_message_id=? AND local_attachment_id=?",
                    params![message_id, action.attachment_id],
                )
                    .await
                    .inspect_err(|e| error!("Failed to remove attachment from message: {e}"))?;
        }
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut writer_guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::LostContext)?;
        let r = action
            .apply_remote_impl(&ctx, &mut writer_guard)
            .await
            .map_err(|e| match e {
                MailContextError::Api(ApiServiceError::Timeout(s)) => {
                    warn!("Attachment upload timed out: {s}");
                    AttachmentUploadError::Timeout.into()
                }
                e => e,
            });

        if let Err(e) = &r
            && let Err(e) = action
                .save_attachment_upload_result(&mut writer_guard, e)
                .await
        {
            error!("Failed to save attachment upload result: {e:?}");
        }
        r
    }
}

impl AttachmentUpload {
    async fn apply_remote_impl(
        &self,
        ctx: &MailUserContext,
        writer_guard: &mut WriterGuard<'_>,
    ) -> Result<<Self as Action>::RemoteOutput, <Self as Action>::Error> {
        let local_message_id = self.local_message_id(writer_guard.tether()).await?;

        let Some(remote_message_id) =
            Message::local_id_counterpart(local_message_id, writer_guard.tether()).await?
        else {
            return Err(
                AttachmentUploadError::MessageDoesNotExistOnServer(local_message_id).into(),
            );
        };

        // Get the attachment.
        let Some(mut attachment) =
            Attachment::find_by_id(self.attachment_id, writer_guard.tether()).await?
        else {
            return Err(AttachmentUploadError::AttachmentDataMissing(self.attachment_id).into());
        };

        if let Some(remote_id) = attachment.remote_id() {
            tracing::warn!("{:?} is already synced ({})", self.attachment_id, remote_id);
            return Ok(());
        }

        encrypt_and_upload_attachment(
            ctx,
            self.metadata_id,
            &self.address_id,
            local_message_id,
            &remote_message_id,
            &mut attachment,
            writer_guard,
        )
        .await?;
        Ok(())
    }

    async fn save_attachment_upload_result(
        &self,
        writer_guard: &mut WriterGuard<'_>,
        error: &MailContextError,
    ) -> Result<(), WriterGuardError> {
        writer_guard
            .tx(async |tx| {
                let mut send_result = DraftSendResult::failure(
                    self.local_message_id.expect("Should be set by now"),
                    DraftSendResultOrigin::AttachmentUpload,
                    DraftSendFailure::from_mail_context_error(error),
                );

                send_result
                    .save(tx)
                    .await
                    .inspect_err(|e| error!("Failed to save send result: {e:?}"))?;

                if let Some(mut attachment_metadata) =
                    DraftAttachmentMetadata::find_by_id(self.attachment_id, tx).await?
                {
                    if error.is_network_failure() {
                        attachment_metadata.set_offline_state();
                    } else {
                        attachment_metadata.set_error_state(
                            DraftAttachmentUploadError::from_mail_context_error(error),
                        );
                    }
                    attachment_metadata.save(tx).await.inspect_err(|e| {
                        error!("Failed to save draft attachment metadata: {e:?}")
                    })?;
                }

                Ok(())
            })
            .await
    }
}

#[tracing::instrument(skip_all, fields(attachment_id = %attachment.id()))]
async fn encrypt_and_upload_attachment(
    ctx: &MailUserContext,
    metadata_id: MetadataId,
    address_id: &AddressId,
    local_message_id: LocalMessageId,
    message_id: &MessageId,
    attachment: &mut Attachment,
    writer_guard: &mut WriterGuard<'_>,
) -> Result<(), MailContextError> {
    // Early check this requirement.
    let new_attachment_disposition = match attachment.disposition {
        Disposition::Attachment => NewAttachmentDisposition::Attachment,
        Disposition::Inline => {
            let Some(content_id) = &attachment.content_id else {
                return Err(AttachmentUploadError::MissingContentId(attachment.id()).into());
            };
            NewAttachmentDisposition::Inline(content_id.clone().into_inner())
        }
    };

    debug!("Retrieving from cache");
    let data = match attachment.content_data(ctx, writer_guard).await {
        Ok(data) => data,
        Err(err) => {
            error!("{err}");
            return Err(AttachmentUploadError::AttachmentDataMissing(attachment.id()).into());
        }
    };

    debug!("Encrypting");
    let encrypted_attachment = Attachment::encrypt(ctx, address_id, &data)
        .await
        .inspect_err(|e| error!("Failed to encrypt attachment: {e:?}"))?;

    debug!("Uploading");
    let new_attachment_params = NewAttachmentParams {
        filename: attachment.filename.clone(),
        message_id: message_id.clone(),
        mime_type: attachment.mime_type.to_string(),
        disposition: new_attachment_disposition,
        key_packets: encrypted_attachment.metadata.key_packets,
        signature: encrypted_attachment.metadata.signature,
        enc_signature: encrypted_attachment.metadata.encrypted_signature,
        data_packet: encrypted_attachment.data,
    };

    let response = match ctx.session().post_attachment(new_attachment_params).await {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to upload attachment: {:?}", e);
            let Some(proton_error) = e.to_proton_error() else {
                return Err(MailContextError::from(e));
            };

            return Err(
                if proton_error.code == Mail::AttachmentMessageAlreadySent as u32 {
                    AttachmentUploadError::MessageAlreadySent.into()
                } else if proton_error.code == Mail::TooManyAttachments as u32 {
                    // backend returns this error for these cases:
                    // * Attachment size > 25 mb
                    // * Total Attachment Size > 25 mb
                    // * Attachment count >= 100
                    // Lets try to guess what it was
                    if attachment.size > Attachment::MAX_ATTACHMENT_SIZE {
                        AttachmentUploadError::AttachmentTooLarge.into()
                    } else if let Ok(counts) =
                        DraftAttachmentMetadata::total_attachments_size_and_count(
                            metadata_id,
                            writer_guard.tether(),
                        )
                        .await
                        .inspect_err(|e| warn!("Failed to load message attachment stats: {e}"))
                    {
                        if counts.total > Attachment::MAX_ATTACHMENTS_PER_MESSAGE {
                            AttachmentUploadError::TooManyAttachments.into()
                        } else if counts.total_size > Attachment::MAX_ATTACHMENT_SIZE {
                            AttachmentUploadError::TotalAttachmentSizeTooLarge.into()
                        } else {
                            // Something else went wrong, lets return default error.
                            AttachmentUploadError::TooManyAttachments.into()
                        }
                    } else {
                        // Default fallback
                        AttachmentUploadError::TooManyAttachments.into()
                    }
                } else {
                    e.into()
                },
            );
        }
    };

    info!("Attachment created with id = {}", response.attachment.id);

    // Update attachment with data returned from the server.
    attachment.attachment_type = AttachmentType::Remote(Some(response.attachment.id));
    attachment.signature = response.attachment.signature.map(Into::into);
    attachment.enc_signature = response.attachment.enc_signature.map(Into::into);
    attachment.key_packets = Some(response.attachment.key_packets.into());
    attachment.size = response.attachment.file_size;
    attachment.image_height = response.attachment.headers.image_height;
    attachment.image_width = response.attachment.headers.image_width;
    attachment.transfer_encoding = response.attachment.headers.content_transfer_encoding;
    attachment.local_message_id = Some(local_message_id);
    attachment.remote_message_id = Some(message_id.clone());

    debug!("Updating database state");
    writer_guard
        .tx::<_, _, MailContextError>(async |tx: &Bond<'_>| {
            // Mark attachment as uploaded.
            let Some(mut draft_attachment_metadata) =
                DraftAttachmentMetadata::find_by_id(attachment.id(), tx).await?
            else {
                return Err(
                    AttachmentUploadError::AttachmentMetadataNotFound(attachment.id()).into(),
                );
            };
            draft_attachment_metadata.set_uploaded_state();
            draft_attachment_metadata
                .save(tx)
                .await
                .inspect_err(|e| error!("Failed to update draft attachment metadata: {e:?}"))?;
            attachment
                .save(tx)
                .await
                .inspect_err(|e| error!("Failed to save attachment: {e:?}"))?;
            Ok(())
        })
        .await?;
    debug!("Done");
    Ok(())
}
