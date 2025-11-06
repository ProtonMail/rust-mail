use crate::MailContextError;
use crate::actions::draft::SEND_ACTION_GROUP;
use crate::datatypes::{LocalAttachmentId, LocalMessageId};
use crate::draft::{AttachmentRemoveError, AttachmentUploadError};
use crate::models::{
    Attachment, AttachmentType, DraftAttachmentMetadata, DraftAttachmentOwnership, DraftMetadata,
    MetadataId,
};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::Deserialize;
use serde_with::serde_derive::Serialize;
use stash::orm::Model as _;
use stash::params;
use stash::stash::{Bond, Tether};
use tracing::{debug, error, info};

#[derive(Serialize, Deserialize)]
pub struct AttachmentRemove {
    metadata_id: MetadataId,
    attachment_id: LocalAttachmentId,
    local_message_id: Option<LocalMessageId>,
}

impl AttachmentRemove {
    pub fn new(metadata_id: MetadataId, attachment_id: LocalAttachmentId) -> Self {
        Self {
            metadata_id,
            attachment_id,
            local_message_id: None,
        }
    }
    async fn local_message_id(
        &self,
        tether: &Tether,
    ) -> Result<Option<LocalMessageId>, MailContextError> {
        let Some(metadata) = DraftMetadata::find_by_id(self.metadata_id, tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", self.metadata_id);
            return Err(AttachmentUploadError::MetadataNotFound(self.metadata_id).into());
        };

        Ok(metadata.local_message_id)
    }
}

impl Action for AttachmentRemove {
    const TYPE: Type = Type("draft_attachment_remove");
    const GROUP: ActionGroup = SEND_ACTION_GROUP;
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Normal;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = AttachmentRemoveHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct AttachmentRemoveHandler {
    pub api: Session,
}

impl Handler for AttachmentRemoveHandler {
    type Action = AttachmentRemove;

    async fn apply_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let Some(mut attachment_metadata) =
            DraftAttachmentMetadata::find_by_id(action.attachment_id, tx)
                .await
                .inspect_err(|e| error!("Failed to load draft attachment metadata: {e:?}"))?
        else {
            return Err(
                AttachmentRemoveError::AttachmentMetadataNotFound(action.attachment_id).into(),
            );
        };

        // find message
        let message_id = action.local_message_id(tx).await?;

        tracing::info!(
            "Removing attachment {} from draft {}",
            attachment_metadata.local_attachment_id,
            attachment_metadata.metadata_id
        );

        // remove attachment from message. It is possible the draft does not exist yet, but
        // the attachment metadata is present nonetheless.
        if let Some(message_id) = message_id {
            tx.execute(
                "DELETE FROM message_attachments WHERE local_message_id=? AND local_attachment_id=?",
                params![message_id, action.attachment_id],
            )
                .await
                .inspect_err(|e| error!("Failed to remove attachment from message: {e}"))?;
        }

        // update attachment action id
        attachment_metadata.action_id = Some(this_id);
        attachment_metadata.deleted = true;
        attachment_metadata
            .save(tx)
            .await
            .inspect_err(|e| error!("Failed to save draft attachment metadata: {e:?}"))?;

        action.local_message_id = message_id;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Restore undeleted status.
        if let Some(mut attachment_metadata) =
            DraftAttachmentMetadata::find_by_id(action.attachment_id, tx)
                .await
                .inspect_err(|e| error!("Failed to load draft attachment metadata: {e:?}"))?
        {
            attachment_metadata.deleted = false;
            attachment_metadata
                .save(tx)
                .await
                .inspect_err(|e| error!("Failed to save draft attachment metadata: {e:?}"))?;
        };
        // re-assign attachment to message.
        if let Some(message_id) = action.local_message_id {
            tx.execute(
                "INSERT OR IGNORE INTO message_attachments (local_message_id, local_attachment_id) VALUES(?,?)",
                params![message_id, action.attachment_id],
            )
                .await.inspect_err(|e| error!("Failed to assign attachment to message: {e}"))?;
        }
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut writer_guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        // check metadata to see if attachment is owned or inherited
        let Some(attachment_metadata) =
            DraftAttachmentMetadata::find_by_id(action.attachment_id, writer_guard.tether())
                .await
                .inspect_err(|e| error!("Failed to load draft attachment metadata: {e:?}"))?
        else {
            return Err(
                AttachmentRemoveError::AttachmentMetadataNotFound(action.attachment_id).into(),
            );
        };

        // if owned delete on the backend
        if matches!(
            attachment_metadata.ownership,
            DraftAttachmentOwnership::Owned
        ) && let Some(AttachmentType::Remote(Some(remote_id))) =
            Attachment::local_id_counterpart(action.attachment_id, writer_guard.tether()).await?
        {
            info!("Deleting {remote_id:?}");

            self.api
                .delete_attachment(remote_id)
                .await
                .inspect_err(|e| error!("Failed to delete attachment on the server{e}"))?;
        }

        // Delete metadata & attachment record
        writer_guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx: &Bond<'_>| {
                // If we own the attachment, delete it.
                if matches!(
                    attachment_metadata.ownership,
                    DraftAttachmentOwnership::Owned
                ) {
                    info!("Deleting {:?} locally", action.attachment_id);
                    Attachment::delete_by_id(action.attachment_id, tx)
                        .await
                        .inspect_err(|e| {
                            error!("Failed to delete attachment: {e:?}");
                        })?;
                }

                debug!("Deleting draft attachment metadata");
                attachment_metadata
                    .delete(tx)
                    .await
                    .inspect_err(|e| error!("Failed to delete draft attachment metadata: {e:?}"))?;

                Ok(())
            })
            .await
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
