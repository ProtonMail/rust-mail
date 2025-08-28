use crate::actions::draft::{
    SEND_ACTION_GROUP, local_all_draft_label_id, local_draft_label_id, local_sent_label_id,
};
use crate::datatypes::LocalMessageId;
use crate::datatypes::{MessageFlags, SystemLabelId};
use crate::draft::UndoError;
use crate::models::Message;
use crate::{AppError, MailContextError};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_core_api::consts::Mail;
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use tracing::{error, info, warn};

/// Action to cancel sending of a sent message.
///
/// This assumes that the user has a send delay configured in their mail settings.
///
/// We locally check if the message is in the sent folder before applying this action as it is
/// expected that this action only be used after a message was sent.
///
#[derive(Serialize, Deserialize)]
pub struct UndoSend {
    id: LocalMessageId,
    remote_id: Option<MessageId>,
}

impl UndoSend {
    /// Create a new instance for message with `id`.
    pub fn new(id: LocalMessageId) -> Self {
        Self {
            id,
            remote_id: None,
        }
    }
}

impl Action for UndoSend {
    const TYPE: Type = Type("undo_send");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Highest;
    const GROUP: ActionGroup = SEND_ACTION_GROUP;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UndoSendHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct UndoSendHandler {
    pub api: Session,
}

impl Handler for UndoSendHandler {
    type Action = UndoSend;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Check if message is in sent folder or outbox + sent flag
        info!("Undo send for {:?}", action.id);

        let Some(mut message) = Message::find_by_id(action.id, tx).await? else {
            return Err(AppError::MessageMissing(action.id).into());
        };

        // Message must have a remote id for this action. Unlike draft action we actually require
        // that the message has been sent before we can actually undo it, which implies it must
        // have a remote id.
        let Some(remote_id) = message.remote_id.clone() else {
            return Err(AppError::MessageHasNoRemoteId(action.id).into());
        };

        // Check that the message can actually be undo sent. It must be in the send folder
        // and have the SENT flag.
        if !(message.label_ids.contains(&LabelId::sent())
            && (message.flags & MessageFlags::SENT == MessageFlags::SENT))
        {
            return Err(UndoError::MessageCanNotBeUndoSent(action.id).into());
        }

        let local_all_draft_label_id = local_all_draft_label_id(tx).await?;
        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_sent_label_id = local_sent_label_id(tx).await?;

        message.flags.remove(MessageFlags::SENT);
        message
            .save(tx)
            .await
            .inspect_err(|e| error!("Failed to remove sent flag: {e:?}"))?;

        // Move message back to drafts
        Message::remove_label(local_sent_label_id, [action.id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove sent label: {e:?}"))?;

        Message::apply_label(local_draft_label_id, [action.id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply draft label: {e:?}"))?;

        Message::apply_label(local_all_draft_label_id, [action.id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply all draft label: {e:?}"))?;

        action.remote_id = Some(remote_id);
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let Some(mut message) = Message::find_by_id(action.id, tx).await? else {
            warn!("Message not found: {}", action.id);
            return Ok(());
        };

        let local_all_draft_label_id = local_all_draft_label_id(tx).await?;
        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_sent_label_id = local_sent_label_id(tx).await?;
        message.flags.set(MessageFlags::SENT, true);
        message.time = UnixTimestamp::now();
        message
            .save(tx)
            .await
            .inspect_err(|e| error!("Failed to add sent flag: {e:?}"))?;

        Message::remove_label(local_draft_label_id, [action.id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove draft label: {e:?}"))?;

        Message::remove_label(local_all_draft_label_id, [action.id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove all draft label: {e:?}"))?;

        Message::apply_label(local_sent_label_id, [action.id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply send label: {e:?}"))?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let remote_id = action
            .remote_id
            .clone()
            .expect("remote id should not be None");

        info!("Undo sending {:?}", remote_id);

        let response = match self.api.cancel_send(remote_id.clone()).await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to cancel send: {e:?}");
                if let Some(proton_error) = e.to_proton_error()
                    && proton_error.code == Mail::MessageSentCanNoLongerBeUndone as u32
                {
                    return Err(UndoError::SendCanNoLongerBeUndone.into());
                }
                return Err(e.into());
            }
        };

        guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                let mut message = Message::from_api_metadata(response.message, tx)
                    .await
                    .inspect_err(|e| error!("Failed to convert remote metadata:{e:?}"))?;

                message
                    .save(tx)
                    .await
                    .inspect_err(|e| error!("Failed to save update message: {e:?}"))?;
                Ok(())
            })
            .await
    }
}
