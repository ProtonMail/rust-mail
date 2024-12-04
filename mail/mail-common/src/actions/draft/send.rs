use crate::actions::draft::{load_message_body, local_draft_label_id};
use crate::datatypes::MessageFlags;
use crate::draft::send::{
    build_packages, load_all_recipients, load_send_preferences_for_recipients,
};
use crate::draft::{Error, ReplyMode};
use crate::models::{
    Conversation, DraftMetadata, MailSettings, Message, MessageBodyMetadata, MetadataId,
};
use crate::{AppError, MailContextError, MailUserContext};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::LocalId;
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

#[derive(Serialize, Deserialize)]
pub struct Send {
    metadata_id: MetadataId,
    local_message_id: Option<LocalId>,
}

impl Send {
    pub fn new(metadata_id: MetadataId) -> Self {
        Self {
            metadata_id,
            local_message_id: None,
        }
    }
}

impl Action for Send {
    const TYPE: Type = Type("send_draft");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = SendHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct SendHandler;

impl proton_action_queue::action::Handler for SendHandler {
    type Action = Send;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_sent_label_id = crate::actions::draft::local_sent_label_id(tx).await?;

        let Some(metadata) = DraftMetadata::find_by_id(action.metadata_id, tx)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e}");
            })?
        else {
            error!("Could not find metadata {}", action.metadata_id);
            return Err(Error::MetadataNotFound(action.metadata_id).into());
        };

        let Some(local_message_id) = metadata.local_message_id else {
            error!("The Draft does not have message yet");
            return Err(Error::DraftWithoutMessage.into());
        };

        let Some(mut message) = Message::find_by_id(local_message_id, tx)
            .await
            .inspect_err(|e| error!("Failed to load message: {e}"))?
        else {
            error!("Could not find draft message {}", local_message_id);
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        message.flags.set(MessageFlags::SENT, true);
        message.save(tx).await.inspect_err(|e| {
            error!("Failed to update message sent flag: {e}");
        })?;

        Message::move_messages(
            local_draft_label_id,
            local_sent_label_id,
            vec![local_message_id],
            tx,
        )
        .await
        .inspect_err(|e| error!("Failed to move draft into sent folder: {e}"))?;

        action.local_message_id = Some(local_message_id);

        Ok(())
    }

    async fn revert_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let local_message_id = action.local_message_id.expect("Should be set");
        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_sent_label_id = crate::actions::draft::local_sent_label_id(tx).await?;

        let Some(mut message) = Message::find_by_id(local_message_id, tx)
            .await
            .inspect_err(|e| error!("Failed to load message: {e}"))?
        else {
            error!("Could not find draft message {}", local_message_id);
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        message.flags.set(MessageFlags::SENT, false);
        message.save(tx).await.inspect_err(|e| {
            error!("Failed to update message sent flag (revert): {e}");
        })?;

        Message::move_messages(
            local_sent_label_id,
            local_draft_label_id,
            vec![local_message_id],
            tx,
        )
        .await
        .inspect_err(|e| error!("Failed to move draft from sent folder: {e}"))?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        context: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let local_message_id = action.local_message_id.expect("Should be set");
        let tether = stash.connection();
        let Some(draft_metadata) = DraftMetadata::find_by_id(action.metadata_id, &tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e}");
            })?
        else {
            error!("Could not find metadata {}", action.metadata_id);
            return Err(Error::MetadataNotFound(action.metadata_id).into());
        };

        let Some(message_metadata) = Message::find_by_id(local_message_id, &tether).await? else {
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        let Some(remote_message_id) = message_metadata.remote_id.clone() else {
            error!("Draft message {local_message_id} does not have remote id");
            return Err(AppError::MessageHasNoRemoteId(local_message_id).into());
        };

        let Some(message_body_metadata) =
            MessageBodyMetadata::for_message(local_message_id, &tether)
                .await
                .inspect_err(|e| {
                    error!("Failed to load message body metadata for {local_message_id}: {e}")
                })?
        else {
            return Err(AppError::MessageBodyMetadataMissing(local_message_id).into());
        };

        // Load the mail settings of the sending user.
        let mail_settings = MailSettings::get(&tether)
            .await
            .inspect_err(|e| error!("Failed to load mail settings: {e}"))?
            .unwrap_or_default();

        // Load body - it is not encrypted.
        let stored_message_body = load_message_body(context, &message_metadata)?;

        // Get recipient emails.
        let recipient_emails = load_all_recipients(&message_metadata);
        if recipient_emails.is_empty() {
            error!("No recipients associated with the current draft");
            return Err(Error::NoRecipients.into());
        }

        let pgp_provider = new_pgp_provider();

        // Load send preferences for each recipient of the message.
        let send_preferences = load_send_preferences_for_recipients(
            context,
            &pgp_provider,
            &tether,
            &recipient_emails,
            mail_settings.crypto_mail_settings(),
        )
        .await
        .inspect_err(|err| error!("Failed to load send preferences for recipients: {err}"))?;

        // Unlock sender address keys
        let address_keys = context
            .unlocked_address_keys(&pgp_provider, &message_metadata.remote_address_id)
            .await
            .inspect_err(|err| error!("Failed to load address key for sending: {err}"))?;

        // TODO(ET-1407): Load the metadata of the attached attachments.
        let attachments = Vec::new();

        let packages = build_packages(
            context,
            &pgp_provider,
            &address_keys,
            send_preferences,
            &message_body_metadata,
            &stored_message_body,
            &attachments,
        )
        .await
        .map_err(Error::SendMessage)
        .inspect_err(|err| error!("Failed build packages for recipients: {err}"))?;

        let auto_save_contacts = Some(mail_settings.auto_save_contacts);

        let response = context
            .api()
            .send_mail(remote_message_id.into(), packages, auto_save_contacts)
            .await
            .inspect_err(|err| {
                error!("Failed to send send email request: {err}");
            })?;

        // Update conversation
        let tx = tether.transaction().await?;
        let mut conversation: Conversation = response.conversation.into();
        conversation
            .save(&tx)
            .await
            .inspect_err(|err| error!("Failed to update conversation after send: {err}"))?;

        // Update message and body metadata
        let (mut metadata, mut body_metadata, _) = Message::from_api_data(response.sent, &tx)
            .await
            .inspect_err(|e| {
                error!("Failed to convert message from API response: {e}");
            })?;

        metadata.save(&tx).await.inspect_err(|e| {
            error!("Failed to update message metadata after send: {e}");
        })?;

        body_metadata.save(&tx).await.inspect_err(|e| {
            error!("Failed to update message body metadata after send: {e}");
        })?;

        // Update parent message's send flag. Only do this here since
        // one message can be replied to/forwarded many times and undoing this
        // can produce incorrect results.
        if let (Some(parent_id), Some(reply_mode)) =
            (draft_metadata.local_parent_id, draft_metadata.reply_mode)
        {
            if let Some(mut parent_message) = Message::find_by_id(parent_id, &tx)
                .await
                .inspect_err(|e| error!("Failed to load parent message {parent_id}: {e}"))?
            {
                match reply_mode {
                    ReplyMode::Sender => parent_message.is_replied = true,
                    ReplyMode::All => {
                        parent_message.is_replied_all = true;
                    }
                    ReplyMode::Forward => {
                        parent_message.is_forwarded = true;
                    }
                }
            } else {
                error!("Could not find parent message {parent_id}, perhaps it was deleted?");
            };
        }

        // Delete draft metadata
        DraftMetadata::delete(action.metadata_id, &tx)
            .await
            .inspect_err(|e| error!("Failed to delete draft metadata after send: {e}"))?;

        tx.commit().await?;
        Ok(())
    }
}
