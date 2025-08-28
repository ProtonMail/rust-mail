use crate::actions::ConversationOrMessage;
use crate::actions::draft::{
    SEND_ACTION_GROUP, local_all_draft_label_id, local_all_scheduled_label_id,
    local_draft_label_id, local_outbox_label_id, local_sent_label_id,
};
use crate::datatypes::{LocalMessageId, MessageFlags, RollbackItemType};
use crate::draft::send::{EoData, MailType, build_packages, load_prefs};
use crate::draft::{
    MIN_EXPIRATION_TIME_SECONDS, ReplyMode, SendError, draft_v1,
    draft_v1::draft_attachment_staging_path,
};
use crate::models::{
    Conversation, DraftAttachmentMetadata, DraftMetadata, DraftSendFailure, DraftSendResult,
    DraftSendResultOrigin, MailSettings, Message, MessageBody, MessageCounters, MessageMimeType,
    MetadataId, RollbackItem,
};
use crate::{AppError, MailContextError, MailUserContext, draft};
use chrono::{DateTime, Local};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, FactoryError, FactoryResult, Handler, Priority, Type,
    VersionConverter, VersionConverterError, WriterGuard, WriterGuardError, deserialize,
};
use proton_core_api::consts::Mail;
use proton_core_api::services::proton::PrivateEmail;
use proton_core_api::services::proton::prelude::AddressId;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::keys::ComposerPreference;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, StashError};
use std::collections::HashSet;
use std::sync::Weak;
use std::time::Duration;
use tracing::{error, info};

#[derive(Serialize, Deserialize)]
pub struct Send {
    metadata_id: MetadataId,
    address_id: AddressId,
    local_message_id: Option<LocalMessageId>,
    recipients: Vec<PrivateEmail>,
    mime_type: MessageMimeType,
    #[serde(default)]
    delivery_time: Option<UnixTimestamp>,
}

impl Send {
    pub fn new(draft: &draft_v1::Draft) -> Self {
        Self {
            metadata_id: draft.metadata_id,
            local_message_id: None,
            address_id: draft.address_id.clone(),
            recipients: Self::combine_recipients(draft),
            mime_type: draft.mime_type(),
            delivery_time: None,
        }
    }

    pub fn scheduled(draft: &draft_v1::Draft, delivery_time: DateTime<Local>) -> Self {
        Self {
            metadata_id: draft.metadata_id,
            local_message_id: None,
            address_id: draft.address_id.clone(),
            recipients: Self::combine_recipients(draft),
            mime_type: draft.mime_type(),
            delivery_time: Some(delivery_time.into()),
        }
    }

    fn combine_recipients(draft: &draft_v1::Draft) -> Vec<PrivateEmail> {
        let to_list = draft.to_list.to_message_recipients();
        let cc_list = draft.cc_list.to_message_recipients();
        let bcc_list = draft.bcc_list.to_message_recipients();
        let recipient_emails: HashSet<PrivateEmail> = HashSet::from_iter(
            to_list
                .into_iter()
                .chain(cc_list)
                .chain(bcc_list)
                .map(|value| value.address),
        );

        recipient_emails.into_iter().collect::<Vec<_>>()
    }

    fn is_scheduled(&self) -> bool {
        self.delivery_time.is_some()
    }

    fn update_sent_flag(&self, message: &mut Message, value: bool) {
        if self.is_scheduled() {
            message.flags.set(MessageFlags::SCHEDULED_SEND, value);
        } else {
            message.flags.set(MessageFlags::SENT, value);
        }
    }
}

pub type UndoTimestamp = UnixTimestamp;

const SEND_ACTION_VERSION: u32 = 2;
impl Action for Send {
    const TYPE: Type = Type("send_draft");
    const VERSION: u32 = SEND_ACTION_VERSION;
    const PRIORITY: Priority = Priority::High;
    const GROUP: ActionGroup = SEND_ACTION_GROUP;

    type VersionConverter = SendVersionConverter;
    type Handler = SendHandler;
    type RemoteOutput = (MessageId, UndoTimestamp);
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct SendVersionConverter;

impl VersionConverter for SendVersionConverter {
    type Output = Send;

    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        if current_version > SEND_ACTION_VERSION {
            return Err(FactoryError::VersionConverter(
                VersionConverterError::InvalidVersion(current_version),
            ));
        }
        if old_version <= 2 {
            // deserializing an extra optional is fine when it does not exist.
            Ok(deserialize::<Send>(data)?)
        } else {
            Err(FactoryError::VersionConverter(
                VersionConverterError::InvalidVersion(old_version),
            ))
        }
    }
}

const MAX_SCHEDULED_SEND_COUNT: u64 = 100;

pub struct SendHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler for SendHandler {
    type Action = Send;

    async fn apply_local(
        &self,
        action_id: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        // Get recipient emails.
        if action.recipients.is_empty() {
            error!("No recipients associated with the current draft");
            return Err(SendError::NoRecipients.into());
        }

        info!(
            "Sending draft {} (scheduled={})",
            action.metadata_id,
            action.is_scheduled()
        );

        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_outbox_label_id = local_outbox_label_id(tx).await?;
        let local_all_draft_label_id = local_all_draft_label_id(tx).await?;
        let local_all_scheduled_label_id = local_all_scheduled_label_id(tx).await?;

        if action.is_scheduled()
            && let Some(counters) =
                MessageCounters::find_by_id(local_all_scheduled_label_id, tx).await?
            && counters.total >= MAX_SCHEDULED_SEND_COUNT
        {
            return Err(SendError::ScheduleSendMessageLimitExceeded.into());
        }

        let Some(mut metadata) = DraftMetadata::find_by_id(action.metadata_id, tx)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", action.metadata_id);
            return Err(SendError::MetadataNotFound(action.metadata_id).into());
        };

        let Some(local_message_id) = metadata.local_message_id else {
            error!("The Draft does not have message yet");
            return Err(SendError::LocalDraftWithoutMessage.into());
        };

        let Some(mut message) = Message::find_by_id(local_message_id, tx)
            .await
            .inspect_err(|e| error!("Failed to load message: {e:?}"))?
        else {
            error!("Could not find draft message {:?}", local_message_id);
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        action.update_sent_flag(&mut message, true);
        // When schedule sending the time of the message is the expected delivery time.
        if let Some(delivery_time) = action.delivery_time {
            message.time = delivery_time;
        };
        message.save(tx).await.inspect_err(|e| {
            error!("Failed to update message sent flag: {e:?}");
        })?;

        Message::remove_label_async(local_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove draft label: {e:?}"))?;
        Message::remove_label_async(local_all_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to remove all draft label: {e:?}"))?;
        if action.is_scheduled() {
            Message::apply_label_async(local_all_scheduled_label_id, [local_message_id], tx)
                .await
                .inspect_err(|e| error!("Failed to apply scheduled label: {e:?}"))?;
        } else {
            Message::apply_label_async(local_outbox_label_id, [local_message_id], tx)
                .await
                .inspect_err(|e| error!("Failed to apply outbox label: {e:?}"))?;
        }

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
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let local_message_id = action.local_message_id.expect("Should be set");
        let local_draft_label_id = local_draft_label_id(tx).await?;
        let local_outbox_label_id = local_outbox_label_id(tx).await?;
        let local_all_draft_label_id = local_all_draft_label_id(tx).await?;
        let local_all_scheduled_label_id = local_all_scheduled_label_id(tx).await?;

        let Some(mut message) = Message::find_by_id(local_message_id, tx)
            .await
            .inspect_err(|e| error!("Failed to load message: {e:?}"))?
        else {
            error!("Could not find draft message {:?}", local_message_id);
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        action.update_sent_flag(&mut message, false);
        message.time = UnixTimestamp::now();
        message.save(tx).await.inspect_err(|e| {
            error!("Failed to update message sent flag (revert): {e:?}");
        })?;

        if action.is_scheduled() {
            Message::remove_label_async(local_all_scheduled_label_id, [local_message_id], tx)
                .await
                .inspect_err(|e| error!("Failed to remove scheduled label: {e:?}"))?;
        } else {
            Message::remove_label_async(local_outbox_label_id, [local_message_id], tx)
                .await
                .inspect_err(|e| error!("Failed to remove outbox label: {e:?}"))?;
        }
        Message::apply_label_async(local_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply draft label: {e:?}"))?;
        Message::apply_label_async(local_all_draft_label_id, [local_message_id], tx)
            .await
            .inspect_err(|e| error!("Failed to apply all draft label: {e:?}"))?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::LostContext)?;
        let r = Send::apply_remote_impl(&ctx, action, &mut guard).await;

        if let Err(e) = save_send_status(action, &mut guard, &r).await {
            error!("Failed to save draft send result: {e:?}");
        }

        r
    }
}

const SEND_DELIVERY_DELTA_INTERVAL: Duration = Duration::from_secs(60);

impl Send {
    async fn apply_remote_impl(
        ctx: &MailUserContext,
        action: &mut Self,
        guard: &mut WriterGuard<'_>,
    ) -> Result<<Self as Action>::RemoteOutput, <Self as Action>::Error> {
        let local_message_id = action.local_message_id.expect("Should be set");
        let session_encryption_key = ctx.core_context().get_encryption_key()?;

        if let Some(delivery_time) = action.delivery_time {
            let current_time_stamp: UnixTimestamp =
                (UnixTimestamp::now().as_u64() + SEND_DELIVERY_DELTA_INTERVAL.as_secs()).into();

            if current_time_stamp > delivery_time {
                error!(
                    "Unable to schedule sending of message {local_message_id}: schedule date is past"
                );
                return Err(SendError::ScheduleSendExpired.into());
            }
        }

        let local_outbox_label_id = local_outbox_label_id(guard.tether()).await?;
        let local_sent_label_id = local_sent_label_id(guard.tether()).await?;
        let local_all_scheduled_id = local_all_scheduled_label_id(guard.tether()).await?;

        // Check if there are any new attachments that have not yet finished loading.
        if DraftAttachmentMetadata::has_unsynced_attachments(action.metadata_id, guard.tether())
            .await?
        {
            error!("Draft has attachments that have not been uploaded");
            return Err(SendError::MissingAttachmentUploads.into());
        }

        let Some(draft_metadata) = DraftMetadata::find_by_id(action.metadata_id, guard.tether())
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", action.metadata_id);
            return Err(SendError::MetadataNotFound(action.metadata_id).into());
        };

        let expiration_time = draft_metadata.expiration_time().to_optional_timestamp();

        if let Some(expiration_time) = expiration_time {
            let now = UnixTimestamp::now().saturating_add(MIN_EXPIRATION_TIME_SECONDS);
            if expiration_time < now {
                return Err(SendError::ExpirationTimeTooSoon.into());
            }
        }

        let Some(message_metadata) = Message::find_by_id(local_message_id, guard.tether()).await?
        else {
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        let Some(remote_message_id) = message_metadata.remote_id.clone() else {
            error!("Draft message {local_message_id:?} does not have remote id");
            return Err(AppError::MessageHasNoRemoteId(local_message_id).into());
        };

        // check if the message was not already sent by another session. If it was the body
        // is reset and will be lost. Rather than failing later, gracefully exit now.
        // We can not do this check for schedule sent as the message has already been moved
        // the all send label, whereas the regular sent goes to outbox first.
        if message_metadata.is_sent() {
            info!("Message was already sent from another session");
            guard
                .tx::<_, (), WriterGuardError>(async |tx| {
                    Ok(on_already_sent(action.metadata_id, None, tx).await?)
                })
                .await?;
            return Ok((remote_message_id, 0.into()));
        }

        // Load the mail settings of the sending user.
        let mail_settings = MailSettings::get(guard.tether())
            .await
            .inspect_err(|e| error!("Failed to load mail settings: {e:?}"))?
            .unwrap_or_default();

        // Load body - it is not encrypted.
        let Some(stored_message_body) =
            MessageBody::load(message_metadata.id(), guard.tether()).await?
        else {
            return Err(SendError::MessageBodyMissing(message_metadata.id()).into());
        };

        // If the user selects encrypt with password, the password should appear here.
        // The send preference logic will decide for each email if password encryption should be applied.
        // It will only use the password if the recipient is external and has no encryption.
        let eo_data: Option<EoData> =
            draft_metadata
                .to_eo_data(&session_encryption_key)
                .map_err(|e| match e {
                    MailContextError::Draft(draft::Error::Password(
                        draft::PasswordError::Decryption,
                    )) => SendError::EOPasswordDecrypt.into(),
                    e => e,
                })?;

        let pgp = new_pgp_provider();

        // Composer preference to compute the recipent send preference from.
        let composer_preference = ComposerPreference {
            encrypt_to_outside: eo_data.is_some(),
            composer_body_mime_type: stored_message_body.mime_type.into(),
        };

        // Load send preferences for each recipient of the message.
        let send_preferences = load_prefs(
            ctx,
            &pgp,
            guard,
            &action.recipients,
            mail_settings.crypto_mail_settings(),
            composer_preference,
        )
        .await
        .inspect_err(|err| error!("Failed to load send preferences for recipients: {err:?}"))?;

        // Unlock sender address keys
        let address_keys = ctx
            .unlocked_address_keys(&pgp, guard.tether(), &message_metadata.remote_address_id)
            .await
            .inspect_err(|err| error!("Failed to load address key for sending: {err:?}"))?;

        let attachments =
            DraftAttachmentMetadata::attachment_for_draft(action.metadata_id, guard.tether())
                .await
                .inspect_err(|e| error!("Failed to load draft attachments : {e:?}"))?;

        let packages = build_packages(
            ctx,
            MailType::Draft,
            &pgp,
            &address_keys,
            send_preferences,
            action.mime_type.into(),
            &stored_message_body.body,
            &attachments,
            eo_data,
            guard,
        )
        .await
        .map_err(SendError::SendMessage)
        .inspect_err(|err| error!("Failed build packages for recipients: {err:?}"))?;

        let auto_save_contacts = Some(mail_settings.auto_save_contacts);

        info!("Sending {:?}", remote_message_id);

        let delivery_time = match ctx
            .session()
            .send_mail(
                remote_message_id.clone(),
                packages,
                auto_save_contacts,
                Some(Duration::from_secs(mail_settings.delay_send_seconds as u64)),
                action.delivery_time.map(|v| v.as_u64()),
                expiration_time.map(|v| v.as_u64()),
            )
            .await
        {
            Ok(response) => {
                // Update conversation
                guard
                    .tx::<_, _, <Self as Action>::Error>(async |tx| {
                        info!("Message sent/scheduled");
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
                        Message::remove_label_async(local_outbox_label_id, [local_message_id], tx)
                            .await
                            .inspect_err(|e| error!("Failed to remove outbox label: {e:?}"))?;
                        if action.is_scheduled() {
                            Message::apply_label_async(local_all_scheduled_id, [local_message_id], tx)
                                .await
                                .inspect_err(|e| error!("Failed to apply all scheduled label: {e:?}"))?;
                        } else {
                            Message::apply_label_async(local_sent_label_id, [local_message_id], tx)
                                .await
                                .inspect_err(|e| error!("Failed to apply sent label: {e:?}"))?;
                        }
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
                    info!("Message already sent on server");
                    // When the message is already sent, we just need to delete the
                    // metadata. The event loop will take care of the rest.
                    guard
                        .tx::<_, _, <Self as Action>::Error>(async |tx| {
                            Ok(on_already_sent(
                                action.metadata_id,
                                Some(remote_message_id.clone()),
                                tx,
                            )
                            .await?)
                        })
                        .await?;
                    // We have no delivery time here, so we just return 0 to "cancel"
                    // all the checks that depend on this time in the future.
                    0
                } else if proton_error.code == Mail::ExpirationTimeTooSoon as u32 {
                    return Err(SendError::ExpirationTimeTooSoon.into());
                } else {
                    error!("Failed to send send email request: {err:?}");
                    return Err(err.into());
                }
            }
        };

        // try to delete staging path.
        let staging_path = draft_attachment_staging_path(ctx, action.metadata_id);

        if let Err(e) = tokio::fs::remove_dir_all(&staging_path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            // This is a warning as the background process will try again.
            tracing::warn!("Failed to remove staging path: {e:?}");
        }

        Ok((remote_message_id, delivery_time.into()))
    }
}

// Simple wrapper function to catch errors
async fn save_send_status(
    action: &Send,
    guard: &mut WriterGuard<'_>,
    status: &Result<<Send as Action>::RemoteOutput, MailContextError>,
) -> Result<(), WriterGuardError> {
    let origin = if action.is_scheduled() {
        DraftSendResultOrigin::ScheduleSend
    } else {
        DraftSendResultOrigin::Send
    };
    let mut send_result = match status {
        Ok((remote_id, delivery_time)) => DraftSendResult::success(
            action.local_message_id.expect("Should be set"),
            remote_id.clone(),
            *delivery_time,
            origin,
        ),
        Err(error) => {
            let error = DraftSendFailure::from_mail_context_error(error);
            if error.is_skippable() {
                return Ok(());
            } else {
                DraftSendResult::failure(
                    action.local_message_id.expect("Should be set by now"),
                    origin,
                    error,
                )
            }
        }
    };
    // We need to manually set this as it is possible the draft metadata has already been
    // wiped at this point.
    send_result.has_send_action = true;

    guard
        .tx::<_, _, WriterGuardError>(async |tx| Ok(send_result.save(tx).await?))
        .await
}

async fn on_already_sent(
    metadata_id: MetadataId,
    message_id: Option<MessageId>,
    tx: &Bond<'_>,
) -> Result<(), StashError> {
    DraftMetadata::delete(metadata_id, tx)
        .await
        .inspect_err(|e| error!("Failed to delete draft metadata after send: {e:?}"))?;

    if let Some(message_id) = message_id {
        // Register rollback item just in case the event loop already ran
        // and the event was missed.
        RollbackItem::new(message_id.into_inner(), RollbackItemType::Message)
            .save(tx)
            .await
            .inspect_err(|e| error!("Failed to register rollback item: {e:?}"))?;
    }
    Ok(())
}
