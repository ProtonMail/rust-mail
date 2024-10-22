use crate::cache::CacheMessageKey;
use crate::draft::{Draft, Error, ReplyMode};
use crate::models::{Conversation, Message, MessageBodyMetadata, NewDraftMetadata};
use crate::{AppError, MailContextError, MailUserContext};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::Session;
use proton_api_mail::services::proton::request_data::DraftAction;
use proton_core_common::datatypes::{LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::params;
use stash::stash::{Interface, Stash, Tether};
use std::io::Read;
use tracing::error;

/// Action which creates a draft on the server.
///
/// When the draft is successfully created, the remote ids for
/// the conversation and message are updated.
///
/// If the action failed, nothing is reverted.
#[derive(Serialize, Deserialize)]
pub struct Create {
    reply_mode: Option<(ReplyMode, LocalId)>,
    message_id: Option<LocalId>,
    conversation_id: Option<LocalId>,
    address_id: Option<RemoteId>,
}

impl Create {
    pub fn empty() -> Self {
        Self {
            reply_mode: None,
            message_id: None,
            conversation_id: None,
            address_id: None,
        }
    }

    pub fn reply(reply_mode: ReplyMode, message_id: LocalId) -> Self {
        Self {
            reply_mode: Some((reply_mode, message_id)),
            message_id: None,
            conversation_id: None,
            address_id: None,
        }
    }
}

impl Action for Create {
    const TYPE: Type = Type("create_draft");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = RemoteId;

    type LocalOutput = Draft;
    type Error = MailContextError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Create;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        ctx: &MailUserContext,
        action: &mut Self::Action,
        tether: &Tether,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let draft = if let Some((reply_mode, message_id)) = action.reply_mode {
            Draft::reply(ctx, message_id, reply_mode, tether).await
        } else {
            Draft::empty(ctx, tether).await
        }?;

        action.address_id = Some(draft.address_id.clone());
        action.message_id = Some(draft.message_id);
        action.conversation_id = Some(draft.conversation_id);
        Ok(draft)
    }

    async fn revert_local(
        &self,
        _: &MailUserContext,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do - We don't want to remove the existing data unless
        // the user explicitly deletes the draft.
        Ok(())
    }

    async fn apply_remote(
        &self,
        ctx: &MailUserContext,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let tether = stash.connection();

        let message_id = action.message_id.expect("Should be set");
        let conversation_id = action.conversation_id.expect("Should be set");

        let Some(draft_metadata) = NewDraftMetadata::find_by_id(message_id, &tether).await? else {
            return Err(Error::CreateMetadataNotFound(message_id).into());
        };

        // Load all dependencies to make sure they are up to date. For drafts
        // this is fine so we can always access the latest value of the data
        // without having to queue multiple actions.
        let Some(mut message) = Message::find_by_id(message_id, &tether).await? else {
            return Err(AppError::MessageMissing(message_id).into());
        };

        let Some(mut conversation) = Conversation::find_by_id(conversation_id, &tether).await?
        else {
            return Err(AppError::ConversationNotFound(conversation_id).into());
        };

        let Some(mut message_body_metadata) = MessageBodyMetadata::find_first(
            "WHERE local_message_id=?",
            params![message_id],
            &tether,
        )
        .await?
        else {
            return Err(AppError::MessageMissing(message_id).into());
        };

        // Load body.
        let key = CacheMessageKey::from_message(&message, &tether);
        let Some(mut message_body_reader) = ctx.messages_cache().get_item(&key)? else {
            return Err(AppError::MessageMissing(message_id).into());
        };

        let mut message_body = String::with_capacity(usize::try_from(message.size).unwrap_or(0));
        message_body_reader.read_to_string(&mut message_body)?;

        // Create draft on the server.
        let new_message = Draft::remote_create(
            ctx,
            session,
            action.address_id.clone().expect("Should be set"),
            draft_metadata
                .reply_mode
                .map_or(DraftAction::Reply, Into::into),
            &message,
            &message_body_metadata,
            &message_body,
            draft_metadata.remote_parent_id,
        )
        .await
        .inspect_err(|e| {
            error!("Failed to create draft on remote: {e}");
        })?;

        // Note: This section will be generalized as part of ET-1353 when
        // we implement draft updates.
        tether.transaction().await?;
        let row_id = message.row_id;

        // Update remote ids
        message.remote_id = Some(new_message.metadata.id.clone().into());
        message.remote_conversation_id = Some(new_message.metadata.conversation_id.clone().into());
        conversation.remote_id = Some(new_message.metadata.conversation_id.clone().into());

        // Update message metadata
        message_body_metadata.remote_message_id = message.remote_id.clone();
        message_body_metadata.header = new_message.header.clone();
        message_body_metadata.parsed_headers.headers = new_message.parsed_headers.clone();

        // Update conversation
        conversation
            .save_using(&tether)
            .await
            .inspect_err(|e| error!("Failed to update the conversation: {e}"))?;

        // Update message data
        message = Message::from_api_data(new_message, &tether)
            .await
            .inspect_err(|e| {
                error!("Failed to convert api message: {e}");
            })?;
        message.row_id = row_id;
        message.local_id = Some(message_id);
        message.save_using(&tether).await.inspect_err(|e| {
            error!("Failed to update the message: {e}");
        })?;

        // Update body metadata
        message_body_metadata
            .save_using(&tether)
            .await
            .inspect_err(|e| {
                error!("Failed to update message body metadata: {e}");
            })?;

        // Delete the draft metadata.
        NewDraftMetadata::delete(message_id, &tether)
            .await
            .inspect_err(|e| {
                error!("Failed to remove create metadata: {e}");
            })?;

        tether.commit().await?;

        Ok(message.remote_id.unwrap())
    }
}
