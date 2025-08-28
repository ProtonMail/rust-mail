use crate::MailContextError;
use crate::actions::draft::SEND_ACTION_GROUP;
use crate::datatypes::SystemLabelId;
use crate::datatypes::{LocalConversationId, LocalMessageId};
use crate::draft::DiscardError;
use crate::models::{Conversation, DraftMetadata, Message, MetadataId};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
};
use proton_core_api::consts::General;
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{debug, error, info};

/// Action which discards a Draft.
///
/// Discarding a Draft is equivalent to perma-deleting the message.
///
#[derive(Serialize, Deserialize)]
pub struct Discard {
    metadata_id: MetadataId,
    local_message_id: Option<LocalMessageId>,
    local_conversation_id: Option<LocalConversationId>,
}

impl Discard {
    pub fn new(metadata_id: MetadataId) -> Self {
        Self {
            metadata_id,
            local_message_id: None,
            local_conversation_id: None,
        }
    }
}

impl Action for Discard {
    const TYPE: Type = Type("discard_draft");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::High;
    const GROUP: ActionGroup = SEND_ACTION_GROUP;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = DiscardHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct DiscardHandler {
    pub api: Session,
}

impl Handler for DiscardHandler {
    type Action = Discard;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        info!("Discarding draft {}", action.metadata_id);

        let Some(metadata) = DraftMetadata::find_by_id(action.metadata_id, bond)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", action.metadata_id);
            return Err(DiscardError::MetadataNotFound(action.metadata_id).into());
        };

        if let Some(local_message_id) = metadata.local_message_id {
            debug!("Local message is present, marking as deleted.");
            Message::mark_deleted(vec![local_message_id], bond)
                .await
                .inspect_err(|e| error!("Failed to mark message as deleted: {e:?}"))?;
        }

        DraftMetadata::delete(action.metadata_id, bond)
            .await
            .inspect_err(|e| error!("Failed to delete metadata: {e:?}"))?;

        action.local_message_id = metadata.local_message_id;
        action.local_conversation_id = metadata.local_conversation_id;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Only undo the delete of the message. The draft metadata can be re-created on
        // the next draft open call.
        if let Some(local_message_id) = action.local_message_id {
            Message::mark_undeleted(vec![local_message_id], bond)
                .await
                .inspect_err(|e| error!("Failed to mark message undeleted: {e:?}"))?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let Some(local_message_id) = action.local_message_id else {
            // if there is no local message id, we never create a message and there is
            // nothing to do.
            return Ok(());
        };

        let Some(message_id) =
            Message::local_id_counterpart(local_message_id, guard.tether()).await?
        else {
            return guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    info!("No server state, deleting locally only");
                    // No remote id, we can't issue the request, we should only delete the local data.
                    Message::delete_by_id(local_message_id, tx)
                        .await
                        .inspect_err(|e| {
                            error!("Failed to delete message {local_message_id:?}:{e:?}")
                        })?;

                    // If we are not replying or forwarding, it means we have a new draft and we may
                    // have to delete the conversation id as well.
                    if let Some(local_conversation_id) = action.local_conversation_id
                        && let Some(conversation) =
                            Conversation::find_by_id(local_conversation_id, tx).await?
                    {
                        // Conversation has no remote id, so we need to do local cleanup, but only
                        // if it only has no more messages.
                        if conversation.num_messages == 0 && !conversation.is_synced() {
                            Conversation::delete_by_id(local_conversation_id, tx)
                                .await
                                .inspect_err(|e| {
                                    error!(
                                        "Failed to delete conversation {}:{e}",
                                        local_conversation_id
                                    )
                                })?;
                        }
                    }
                    Ok(())
                })
                .await;
        };

        // Server will take care of deleting orphaned conversations, we do not have
        // to do anything.
        info!("Deleting {message_id:?}");

        let response = self
            .api
            .put_messages_delete(vec![message_id.clone()], Some(LabelId::drafts()))
            .await
            .inspect_err(|e| error!("Failed to delete message on server: {e:?}"))?;

        for result in response.responses {
            if result.id == message_id && result.response.code != General::NoError as u32 {
                error!("Failed to delete message: {:?}", result.response);
                return Err(MailContextError::Draft(DiscardError::DeleteFailed.into()));
            }
        }

        // Nothing else to do, event loop will take care of the cleanup.
        Ok(())
    }
}
