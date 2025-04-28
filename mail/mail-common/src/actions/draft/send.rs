use crate::actions::draft::{
    SEND_ACTION_GROUP, local_all_draft_label_id, local_draft_label_id, local_outbox_label_id,
    local_sent_label_id,
};
use crate::datatypes::{LocalMessageId, MessageFlags, MimeType, RollbackItemType};
use crate::draft::send::{build_packages, load_send_preferences_for_recipients};
use crate::draft::{Draft, ReplyMode, SaveOrSendError, draft_attachment_staging_path};
use crate::models::{
    Conversation, DraftAttachmentMetadata, DraftMetadata, DraftSendFailure, DraftSendResult,
    DraftSendResultOrigin, MailSettings, Message, MetadataId, RollbackItem,
};
use crate::{AppError, MailContextError, MailUserContext};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
    WriterGuardError,
};
use proton_api_core::consts::Mail;
use proton_api_core::services::proton::prelude::AddressId;
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::services::proton::common::MessageId;
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Bond;
use std::collections::HashSet;
use std::time::Duration;
use tracing::{debug, error};

#[derive(Serialize, Deserialize)]
pub struct Send {
    metadata_id: MetadataId,
    address_id: AddressId,
    local_message_id: Option<LocalMessageId>,
    recipients: Vec<String>,
    mime_type: MimeType,
}

impl Send {
    pub fn new(draft: &Draft) -> Self {
        Self {
            metadata_id: draft.metadata_id,
            local_message_id: None,
            address_id: draft.address_id.clone(),
            recipients: Self::combine_recipients(draft),
            mime_type: draft.mime_type(),
        }
    }

    fn combine_recipients(draft: &Draft) -> Vec<String> {
        let to_list = draft.to_list.to_message_recipients();
        let cc_list = draft.cc_list.to_message_recipients();
        let bcc_list = draft.bcc_list.to_message_recipients();
        let recipient_emails: HashSet<String> = HashSet::from_iter(
            to_list
                .into_iter()
                .chain(cc_list)
                .chain(bcc_list)
                .map(|value| value.address),
        );

        recipient_emails.into_iter().collect::<Vec<_>>()
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
        // Get recipient emails.
        if action.recipients.is_empty() {
            error!("No recipients associated with the current draft");
            return Err(SaveOrSendError::NoRecipients.into());
        }

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

        // Check if there are any new attachments that have not yet finished loading.
        if DraftAttachmentMetadata::has_unsynced_attachments(action.metadata_id, guard.tether())
            .await?
        {
            error!("Draft has attachments that have not been uploaded");
            return Err(SaveOrSendError::MissingAttachmentUploads.into());
        }

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

        // Load the mail settings of the sending user.
        let mail_settings = MailSettings::get(guard.tether())
            .await
            .inspect_err(|e| error!("Failed to load mail settings: {e:?}"))?
            .unwrap_or_default();

        // Load body - it is not encrypted.
        let Some(stored_message_body) = Message::load_decrypted_message_body(
            message_metadata.local_id.unwrap(),
            guard.tether(),
        )
        .await?
        else {
            return Err(AppError::MessageBodyMissing(message_metadata.local_id.unwrap()).into());
        };

        let pgp_provider = new_pgp_provider();

        // Load send preferences for each recipient of the message.
        let send_preferences = load_send_preferences_for_recipients(
            context,
            &pgp_provider,
            guard,
            &action.recipients,
            mail_settings.crypto_mail_settings(),
        )
        .await
        .inspect_err(|err| error!("Failed to load send preferences for recipients: {err:?}"))?;

        // Unlock sender address keys
        let address_keys = context
            .unlocked_address_keys(
                &pgp_provider,
                guard.tether(),
                &message_metadata.remote_address_id,
            )
            .await
            .inspect_err(|err| error!("Failed to load address key for sending: {err:?}"))?;

        let attachments =
            DraftAttachmentMetadata::attachment_for_draft(action.metadata_id, guard.tether())
                .await
                .inspect_err(|e| error!("Failed to load draft attachments : {e:?}"))?;

        // TODO(ET-1407): PGP/Embedded attachments
        let packages = build_packages(
            context,
            &pgp_provider,
            &address_keys,
            send_preferences,
            action.mime_type,
            &stored_message_body,
            // Even though we are already passing in the message body metadata we
            // leave this parameter here for when we handle the PGP embedded case.
            &attachments,
            guard,
        )
        .await
        .map_err(SaveOrSendError::SendMessage)
        .inspect_err(|err| error!("Failed build packages for recipients: {err:?}"))?;

        let auto_save_contacts = Some(mail_settings.auto_save_contacts);

        let delivery_time = match context
            .api()
            .send_mail(
                remote_message_id.clone(),
                packages,
                auto_save_contacts,
                Some(Duration::from_secs(mail_settings.delay_send_seconds as u64)),
            )
            .await
        {
            Ok(response) => {
                // Update conversation
                guard
                    .tx::<_, _, <Self as Action>::Error>(async |tx| {
                        let mut conversation: Conversation = response.conversation.into();
                        conversation.save(tx).await.inspect_err(|err| {
                            error!("Failed to update conversation after send: {err:?}")
                        })?;

                        // Update message and body metadata
                        let (mut metadata, mut body_metadata, _) =
                            Message::from_api_data(response.sent, tx)
                                .await
                                .inspect_err(|e| {
                                    error!("Failed to convert message from API response: {e:?}");
                                })?;

                        metadata.save(tx).await.inspect_err(|e| {
                            error!("Failed to update message metadata after send: {e:?}");
                        })?;

                        body_metadata.save(tx).await.inspect_err(|e| {
                            error!("Failed to update message body metadata after send: {e:?}");
                        })?;

                        // Update parent message's send flag. Only do this here since
                        // one message can be replied to/forwarded many times and undoing this
                        // can produce incorrect results.
                        if let (Some(parent_id), Some(reply_mode)) =
                            (draft_metadata.local_parent_id, draft_metadata.reply_mode)
                        {
                            if let Some(mut parent_message) =
                                Message::find_by_id(parent_id, tx).await.inspect_err(|e| {
                                    error!("Failed to load parent message {parent_id:?}: {e:?}")
                                })?
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
                                error!(
                            "Could not find parent message {parent_id:?}, perhaps it was deleted?"
                        );
                            };
                        }

                        // Move message to sent folder
                        Message::remove_label(local_outbox_label_id, [local_message_id], tx)
                            .await
                            .inspect_err(|e| error!("Failed to remove outbox label: {e:?}"))?;
                        Message::apply_label(local_sent_label_id, [local_message_id], tx)
                            .await
                            .inspect_err(|e| error!("Failed to apply sent label: {e:?}"))?;

                        // Delete draft metadata
                        DraftMetadata::delete(action.metadata_id, tx)
                            .await
                            .inspect_err(|e| error!("Failed to delete draft metadata after send: {e:?}"))?;
                        Ok(())
                    })
                    .await?;
                response.delivery_time
            }
            Err(err) => {
                let Some(proton_error) = err.to_proton_error() else {
                    error!("Failed to send send email request: {err:?}");
                    return Err(err.into());
                };
                if proton_error.code == Mail::MessageAlreadySent as u32 {
                    debug!("Message already sent.");
                    // When the message is already sent, we just need to delete the
                    // metadata. The event loop will take care of the rest.
                    guard
                        .tx::<_, _, <Self as Action>::Error>(async |tx| {
                            DraftMetadata::delete(action.metadata_id, tx)
                                .await
                                .inspect_err(|e| {
                                    error!("Failed to delete draft metadata after send: {e:?}")
                                })?;

                            // Register rollback item just in case the event loop already ran
                            // and the event was missed.
                            RollbackItem::new(
                                remote_message_id.clone().into_inner(),
                                RollbackItemType::Message,
                            )
                            .save(tx)
                            .await
                            .inspect_err(|e| error!("Failed to register rollback item: {e:?}"))?;
                            Ok(())
                        })
                        .await?;
                    // We have no delivery time here, so we just return 0 to "cancel"
                    // all the checks that depend on this time in the future.
                    0
                } else {
                    error!("Failed to send send email request: {err:?}");
                    return Err(err.into());
                }
            }
        };

        // try to delete staging path.
        let staging_path = draft_attachment_staging_path(context, action.metadata_id);
        if let Err(e) = tokio::fs::remove_dir_all(&staging_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                // This is a warning as the background process will try again.
                tracing::warn!("Failed to remove staging path: {e:?}");
            }
        }

        Ok((remote_message_id, delivery_time))
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

    guard
        .tx::<_, _, WriterGuardError>(async |tx| Ok(send_result.save(tx).await?))
        .await
}
