use crate::actions::draft::{
    local_all_draft_label_id, local_draft_label_id, local_outbox_label_id, local_sent_label_id,
    SEND_ACTION_GROUP,
};
use crate::datatypes::{LocalMessageId, MessageFlags};
use crate::draft::send::{
    build_packages, load_all_recipients, load_send_preferences_for_recipients,
};
use crate::draft::{ReplyMode, SaveOrSendError};
use crate::models::{
    Conversation, DraftMetadata, DraftSendFailure, DraftSendResult, DraftSendResultOrigin,
    MailSettings, Message, MessageBodyMetadata, MetadataId,
};
use crate::{AppError, MailContextError, MailUserContext};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
    WriterGuardError,
};
use proton_api_core::consts::Mail;
use proton_api_mail::services::proton::common::MessageId;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use std::time::Duration;
use tracing::error;

#[derive(Serialize, Deserialize)]
pub struct Send {
    metadata_id: MetadataId,
    local_message_id: Option<LocalMessageId>,
}

impl Send {
    pub fn new(metadata_id: MetadataId) -> Self {
        Self {
            metadata_id,
            local_message_id: None,
        }
    }
}

pub type UndoTimestamp = u64;

impl Action for Send {
    const TYPE: Type = Type("send_draft");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::High;
    const GROUP: ActionGroup = SEND_ACTION_GROUP;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = SendHandler;
    type RemoteOutput = (MessageId, UndoTimestamp);
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
        action_id: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_outbox_label_id = local_outbox_label_id(tx).await?;
        let local_all_draft_label_id = local_all_draft_label_id(tx).await?;

        let Some(mut metadata) = DraftMetadata::find_by_id(action.metadata_id, tx)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", action.metadata_id);
            return Err(SaveOrSendError::MetadataNotFound(action.metadata_id).into());
        };

        let Some(local_message_id) = metadata.local_message_id else {
            error!("The Draft does not have message yet");
            return Err(SaveOrSendError::LocalDraftWithoutMessage.into());
        };

        let Some(mut message) = Message::find_by_id(local_message_id, tx)
            .await
            .inspect_err(|e| error!("Failed to load message: {e:?}"))?
        else {
            error!("Could not find draft message {:?}", local_message_id);
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        message.flags.set(MessageFlags::SENT, true);
        message.save(tx).await.inspect_err(|e| {
            error!("Failed to update message sent flag: {e:?}");
        })?;

        Message::remove_label(local_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove draft label: {e:?}"))?;
        Message::remove_label(local_all_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove all draft label: {e:?}"))?;
        Message::apply_label(local_outbox_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply outbox label: {e:?}"))?;

        action.local_message_id = Some(local_message_id);

        metadata.send_action_id = Some(action_id);
        metadata
            .save(tx)
            .await
            .inspect_err(|e| error!("Failed to save updated metadata: {e:?}"))?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let local_message_id = action.local_message_id.expect("Should be set");
        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_outbox_label_id = local_outbox_label_id(tx).await?;
        let local_all_draft_label_id = local_all_draft_label_id(tx).await?;

        let Some(mut message) = Message::find_by_id(local_message_id, tx)
            .await
            .inspect_err(|e| error!("Failed to load message: {e:?}"))?
        else {
            error!("Could not find draft message {:?}", local_message_id);
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        message.flags.set(MessageFlags::SENT, false);
        message.save(tx).await.inspect_err(|e| {
            error!("Failed to update message sent flag (revert): {e:?}");
        })?;

        Message::remove_label(local_outbox_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove outbox label: {e:?}"))?;
        Message::apply_label(local_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply draft label: {e:?}"))?;
        Message::apply_label(local_all_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply all draft label: {e:?}"))?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        context: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let r = Send::apply_remote_impl(context, action, &mut guard).await;
        if let Err(e) = save_send_status(action, &mut guard, &r).await {
            error!("Failed to save draft send result: {e:?}");
        }
        r
    }
}

impl Send {
    async fn apply_remote_impl(
        context: &<Self as Action>::Context,
        action: &mut Self,
        guard: &mut WriterGuard<'_>,
    ) -> Result<<Self as Action>::RemoteOutput, <Self as Action>::Error> {
        let local_message_id = action.local_message_id.expect("Should be set");
        let local_outbox_label_id = local_outbox_label_id(guard.tether()).await?;
        let local_sent_label_id = local_sent_label_id(guard.tether()).await?;

        let Some(draft_metadata) = DraftMetadata::find_by_id(action.metadata_id, guard.tether())
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", action.metadata_id);
            return Err(SaveOrSendError::MetadataNotFound(action.metadata_id).into());
        };

        let Some(message_metadata) = Message::find_by_id(local_message_id, guard.tether()).await?
        else {
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        let Some(remote_message_id) = message_metadata.remote_id.clone() else {
            error!("Draft message {local_message_id:?} does not have remote id");
            return Err(AppError::MessageHasNoRemoteId(local_message_id).into());
        };

        let Some(message_body_metadata) =
            MessageBodyMetadata::for_message(local_message_id, guard.tether())
                .await
                .inspect_err(|e| {
                    error!("Failed to load message body metadata for {local_message_id:?}: {e:?}")
                })?
        else {
            return Err(AppError::MessageBodyMetadataMissing(local_message_id).into());
        };

        // Load the mail settings of the sending user.
        let mail_settings = MailSettings::get(guard.tether())
            .await
            .inspect_err(|e| error!("Failed to load mail settings: {e:?}"))?
            .unwrap_or_default();

        // Load body - it is not encrypted.
        let Some(stored_message_body) = Message::load_decrypted_message_body_from_cache(
            context,
            message_metadata.local_id.unwrap(),
        )?
        else {
            return Err(AppError::MessageBodyMissing(message_metadata.local_id.unwrap()).into());
        };

        // Get recipient emails.
        let recipient_emails = load_all_recipients(&message_metadata);
        if recipient_emails.is_empty() {
            error!("No recipients associated with the current draft");
            return Err(SaveOrSendError::NoRecipients.into());
        }

        let pgp_provider = new_pgp_provider();

        let tx = guard.transaction().await?;
        // Load send preferences for each recipient of the message.
        let send_preferences = load_send_preferences_for_recipients(
            context,
            &pgp_provider,
            &tx,
            &recipient_emails,
            mail_settings.crypto_mail_settings(),
        )
        .await
        .inspect_err(|err| error!("Failed to load send preferences for recipients: {err:?}"))?;

        // Unlock sender address keys
        let address_keys = context
            .unlocked_address_keys(&pgp_provider, &tx, &message_metadata.remote_address_id)
            .await
            .inspect_err(|err| error!("Failed to load address key for sending: {err:?}"))?;

        tx.commit().await?;

        // TODO(ET-1407): PGP/Embedded attachments
        let packages = build_packages(
            context,
            &pgp_provider,
            &address_keys,
            send_preferences,
            &message_body_metadata,
            &stored_message_body,
            // Even though we are already passing in the message body metadata we
            // leave this parameter here for when we handle the PGP embedded case.
            &message_body_metadata.attachments,
        )
        .await
        .map_err(SaveOrSendError::SendMessage)
        .inspect_err(|err| error!("Failed build packages for recipients: {err:?}"))?;

        let auto_save_contacts = Some(mail_settings.auto_save_contacts);

        let response = match context
            .api()
            .send_mail(
                remote_message_id,
                packages,
                auto_save_contacts,
                Some(Duration::from_secs(mail_settings.delay_send_seconds as u64)),
            )
            .await
        {
            Ok(response) => response,
            Err(err) => {
                error!("Failed to send send email request: {err:?}");

                if let Some(proton_error) = err.to_proton_error() {
                    if proton_error.code == Mail::MessageAlreadySent as u32 {
                        return Err(SaveOrSendError::AlreadySent.into());
                    }
                }

                return Err(err.into());
            }
        };

        // Update conversation
        let tx = guard.transaction().await?;
        let mut conversation: Conversation = response.conversation.into();
        conversation
            .save(&tx)
            .await
            .inspect_err(|err| error!("Failed to update conversation after send: {err:?}"))?;

        // Update message and body metadata
        let (mut metadata, mut body_metadata, _) = Message::from_api_data(response.sent, &tx)
            .await
            .inspect_err(|e| {
                error!("Failed to convert message from API response: {e:?}");
            })?;

        metadata.save(&tx).await.inspect_err(|e| {
            error!("Failed to update message metadata after send: {e:?}");
        })?;

        body_metadata.save(&tx).await.inspect_err(|e| {
            error!("Failed to update message body metadata after send: {e:?}");
        })?;

        // Update parent message's send flag. Only do this here since
        // one message can be replied to/forwarded many times and undoing this
        // can produce incorrect results.
        if let (Some(parent_id), Some(reply_mode)) =
            (draft_metadata.local_parent_id, draft_metadata.reply_mode)
        {
            if let Some(mut parent_message) = Message::find_by_id(parent_id, &tx)
                .await
                .inspect_err(|e| error!("Failed to load parent message {parent_id:?}: {e:?}"))?
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
                error!("Could not find parent message {parent_id:?}, perhaps it was deleted?");
            };
        }

        // Move message to sent folder
        Message::remove_label(local_outbox_label_id, [local_message_id], &tx)
            .await
            .inspect_err(|e| error!("Failed to remove outbox label: {e:?}"))?;
        Message::apply_label(local_sent_label_id, [local_message_id], &tx)
            .await
            .inspect_err(|e| error!("Failed to apply sent label: {e:?}"))?;

        // Delete draft metadata
        DraftMetadata::delete(action.metadata_id, &tx)
            .await
            .inspect_err(|e| error!("Failed to delete draft metadata after send: {e:?}"))?;

        tx.commit().await?;
        Ok((
            metadata.remote_id.expect("This is valid"),
            response.delivery_time,
        ))
    }
}

// Simple wrapper function to catch errors
async fn save_send_status(
    action: &Send,
    guard: &mut WriterGuard<'_>,
    status: &Result<<Send as Action>::RemoteOutput, MailContextError>,
) -> Result<(), WriterGuardError> {
    let mut send_result = match status {
        Ok((remote_id, delivery_time)) => DraftSendResult::success(
            action.local_message_id.expect("Should be set"),
            remote_id.clone(),
            (*delivery_time).try_into().unwrap_or(0),
        ),
        Err(error) => {
            let error = DraftSendFailure::from_mail_context_error(error);
            if error.is_skippable() {
                return Ok(());
            } else {
                DraftSendResult::failure(
                    action.local_message_id.expect("Should be set by now"),
                    DraftSendResultOrigin::Send,
                    error,
                )
            }
        }
    };

    let tx = guard.transaction().await?;
    send_result.save(&tx).await?;
    Ok(tx.commit().await?)
}
