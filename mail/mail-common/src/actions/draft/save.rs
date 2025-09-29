use crate::actions::ConversationOrMessage;
use crate::actions::draft::{
    SEND_ACTION_GROUP, local_all_draft_label_id, local_all_mail_label_id, local_draft_label_id,
    local_sent_label_id,
};
use crate::datatypes::{
    AttachmentMetadata, Disposition, LocalMessageId, MessageSender, MessageSenders, MimeType,
    RollbackItemType, SystemLabelId,
};
use crate::datatypes::{LocalAttachmentId, LocalConversationId};
use crate::draft::compose::maybe_sanitize;
use crate::draft::recipients::RecipientList;
use crate::draft::{Draft, ReplyMode, SaveError, compose, draft_v1};
use crate::models::{
    Attachment, Conversation, DraftAttachmentMetadata, DraftAttachmentOwnership, DraftMetadata,
    DraftSendFailure, DraftSendResult, DraftSendResultOrigin, Message, MessageBody,
    MessageBodyMetadata, MessageMimeType, MetadataId, RollbackItem,
};
use crate::{AppError, MailContextError, MailUserContext, draft};
use indoc::indoc;
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, FactoryResult, Handler, Priority, Type, VersionConverter,
    VersionConverterError, WriterGuard, WriterGuardError, deserialize,
};
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_crypto_inbox::message::EncryptedDraft;
use proton_mail_api::services::proton::prelude::{
    DraftParams, DraftReplyOrForwardParams, ExternalId,
};
use proton_mail_api::services::proton::request_data::DraftSender;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError};
use std::sync::Weak;
use tracing::{debug, error, info, warn};

/// Action which creates or updates a draft on the server.
///
/// When the draft is successfully created, the remote ids for
/// the conversation and message are updated.
///
/// If the action failed, nothing is reverted.
#[derive(Serialize, Deserialize, Clone)]
pub struct Save {
    metadata_id: MetadataId,
    to_list: RecipientList,
    cc_list: RecipientList,
    bcc_list: RecipientList,
    message_id: Option<LocalMessageId>,
    conversation_id: Option<LocalConversationId>,
    address_id: AddressId,

    /// Unencrypted subject
    subject: String,

    /// Unencrypted body
    body: String,

    mime_type: MessageMimeType,
    parent_id: Option<LocalMessageId>,
    reply_mode: Option<ReplyMode>,
    save_origin: DraftSendResultOrigin,
    attachment_ids: Vec<LocalAttachmentId>,
    external_id: Option<ExternalId>,
    unread: Option<bool>,
    sender: Option<MessageSender>,

    // can be different from the address email when using aliases
    #[serde(default)]
    sender_email: Option<String>,
}

impl Save {
    pub fn new(draft: &draft_v1::Draft, save_origin: DraftSendResultOrigin) -> Self {
        let transformed = maybe_sanitize(draft.mime_type(), draft.body());

        Self {
            metadata_id: draft.metadata_id,
            to_list: draft.to_list.clone(),
            cc_list: draft.cc_list.clone(),
            bcc_list: draft.bcc_list.clone(),
            message_id: None,
            conversation_id: None,
            address_id: draft.address_id.clone(),
            subject: if draft.subject.is_empty() {
                compose::DEFAULT_SUBJECT.to_owned()
            } else {
                draft.subject.clone()
            },
            body: transformed,
            mime_type: draft.mime_type(),
            parent_id: None,
            reply_mode: None,
            save_origin,
            external_id: None,
            attachment_ids: Vec::new(),
            unread: None,
            sender: None,
            sender_email: Some(draft.sender.clone()),
        }
    }

    pub fn crate_draft_params(&self, encrypted_draft: EncryptedDraft) -> DraftParams {
        DraftParams {
            subject: self.subject.clone(),
            unread: self.unread.expect("Should be set at this point"),
            sender: self
                .sender
                .clone()
                .map(|sender| DraftSender {
                    address: sender.address,
                    name: sender.name,
                })
                .expect("Should be set at this point"),
            to_list: compose::recipient_from_message_sender(&self.to_list.to_message_recipients()),
            cc_list: compose::recipient_from_message_sender(&self.cc_list.to_message_recipients()),
            bcc_list: compose::recipient_from_message_sender(
                &self.bcc_list.to_message_recipients(),
            ),
            external_id: self.external_id.clone().map(|id| id.to_string()),
            draft_flags: 0,
            body: encrypted_draft,
            mime_type: MimeType::from(self.mime_type).into(),
        }
    }
}

impl Action for Save {
    const TYPE: Type = Type("save_draft");
    const VERSION: u32 = 2;
    const PRIORITY: Priority = Priority::High;
    const GROUP: ActionGroup = SEND_ACTION_GROUP;

    type VersionConverter = SaveVersionConverter;
    type Handler = SaveHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailContextError;
}

pub struct SaveVersionConverter {}

impl VersionConverter for SaveVersionConverter {
    type Output = Save;

    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        if !(old_version <= 2 && current_version == 2) {
            return Err(VersionConverterError::InvalidVersion(current_version).into());
        }

        Ok(deserialize::<Save>(data)?)
    }
}

pub struct SaveHandler {
    pub ctx: Weak<MailUserContext>,
}

impl Handler for SaveHandler {
    type Action = Save;

    async fn apply_local(
        &self,
        action_id: ActionId,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        info!("Saving Draft {}", action.metadata_id);

        let local_draft_id = local_draft_label_id(bond).await?;
        let local_all_draft_id = local_all_draft_label_id(bond).await?;
        let local_all_mail_id = local_all_mail_label_id(bond).await?;

        let Some(mut metadata) = DraftMetadata::find_by_id(action.metadata_id, bond)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", action.metadata_id);
            return Err(SaveError::MetadataNotFound(action.metadata_id).into());
        };

        tracing::info!("Saving draft {}", action.metadata_id);

        let body_len = action.body.len() as u64;

        let Some(address) = Address::find_by_remote_id(action.address_id.clone(), bond)
            .await
            .inspect_err(|e| error!("Failed to load address: {e:?}"))?
        else {
            error!("Address with remote id {:?} not found.", action.address_id);
            return Err(SaveError::AddressNotFound(action.address_id.clone()).into());
        };

        // This value can potentially be empty when migrating from and older version of
        // the save action, use the address email when that happens.
        let sender_email = action.sender_email.clone().unwrap_or(address.email.clone());

        let attachments = action
            .attachments(bond)
            .await
            .inspect_err(|e| error!("Failed to load attachments: {e:?}"))?;

        debug!("Draft has {} attachments", attachments.len());

        let attachment_metadata = Save::attachment_metadata(&attachments);
        let attachment_ids = attachments.iter().map(|a| a.id()).collect::<Vec<_>>();

        let conversation_id = if let Some(id) = metadata.local_conversation_id {
            let message_count = Conversation::message_count(id, bond).await?;
            // we should only update the conversation subject if there is only one message.
            if message_count == 1 {
                info!("Updating conversation subject");
                Conversation::update_subject(id, action.subject.clone(), bond).await?;
            }
            id
        } else {
            info!("Conversation does not exist, creating");

            let display_order = Conversation::next_display_order(bond)
                .await
                .inspect_err(|e| error!("Failed to get next conversation display order: {e:?}"))?;

            let mut conversation = action.create_new_conversation(
                &address,
                sender_email.clone(),
                display_order,
                body_len,
                attachment_metadata.clone(),
                attachments.len() as u64,
                action.subject.clone(),
            );

            conversation
                .save(bond)
                .await
                .inspect_err(|e| error!("Failed to create new conversation: {e:?}"))?;

            metadata.local_conversation_id = Some(conversation.id());
            conversation.id()
        };

        let time = UnixTimestamp::now();

        let message = if let Some(message_id) = metadata.local_message_id {
            info!("Local message id is set, update");

            let Some(mut message) = Message::find_by_id(message_id, bond)
                .await
                .inspect_err(|e| error!("Failed to load message: {e:?}"))?
            else {
                return Err(AppError::MessageMissing(message_id).into());
            };

            // A draft can only be updated if it is not in the outbox or sent folder.
            if message.label_ids.contains(&LabelId::outbox())
                || message.label_ids.contains(&LabelId::sent())
            {
                error!("Can't update a draft that was already sent");
                return Err(SaveError::AlreadySent.into());
            }

            action.update_message(
                &address,
                sender_email,
                &mut message,
                attachments.len() as u64,
                attachment_metadata,
                body_len,
                time,
            );

            message.save(bond).await.inspect_err(|e| {
                error!("Failed to update draft message: {e:?}");
            })?;

            let Some(mut body_metadata) = MessageBodyMetadata::for_message(message_id, bond)
                .await
                .inspect_err(|e| error!("Failed to load message metadata: {e:?}"))?
            else {
                return Err(AppError::MessageMissing(message_id).into());
            };

            body_metadata.attachments = attachments;
            body_metadata.mime_type = action.mime_type.into();

            body_metadata.save(bond).await.inspect_err(|e| {
                error!("Failed to update draft body metadata: {e:?}");
            })?;

            message
        } else {
            info!("Local message id is not set, creating new draft");

            let display_order = Message::next_display_order(bond)
                .await
                .inspect_err(|e| error!("Failed to get next message display order: {e:?}"))?;

            let mut message = action.create_new_message(
                &address,
                sender_email,
                attachments.len() as u64,
                attachment_metadata,
                body_len,
                time,
                display_order,
            );

            message.local_conversation_id = Some(conversation_id);

            message
                .save(bond)
                .await
                .inspect_err(|e| error!("Failed to save message: {e:?}"))?;

            let mut message_body_metadata = MessageBodyMetadata {
                local_message_id: Some(message.id()),
                remote_message_id: None,
                header: "".to_string(),
                mime_type: action.mime_type.into(),
                parsed_headers: Default::default(),
                attachments,
                reply_to: Default::default(),
                reply_tos: vec![],
            };

            message_body_metadata
                .save(bond)
                .await
                .inspect_err(|e| error!("Failed to save message body metadata: {e:?}"))?;

            Message::apply_label_async(local_draft_id, std::iter::once(message.id()), bond)
                .await
                .inspect_err(|e| {
                    error!("Failed to apply draft label to new message: {e:?}");
                })?;

            Message::apply_label_async(local_all_draft_id, std::iter::once(message.id()), bond)
                .await
                .inspect_err(|e| {
                    error!("Failed to apply all_draft label to new message: {e:?}");
                })?;

            Message::apply_label_async(local_all_mail_id, std::iter::once(message.id()), bond)
                .await
                .inspect_err(|e| {
                    error!("Failed to apply all_mail label to new message: {e:?}");
                })?;

            message
        };

        MessageBody::ok(&action.body, action.mime_type)
            .store(message.id(), bond)
            .await
            .inspect_err(|e| {
                error!("Failed to store draft body in cache :{e:?}");
            })?;

        metadata.local_message_id = Some(message.id());
        metadata.save_action_id = Some(action_id);

        metadata.save(bond).await.inspect_err(|e| {
            error!("Failed to save draft metadata: {e:?}");
        })?;

        action.message_id = metadata.local_message_id;
        action.conversation_id = metadata.local_conversation_id;
        action.reply_mode = metadata.reply_mode;
        action.parent_id = metadata.local_parent_id;
        action.attachment_ids = attachment_ids;
        action.sender = Some(message.sender);
        action.unread = Some(message.unread);

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Don't remove resources if draft failed to create.
        // These items will be removed via a discard action.
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let ctx = self.ctx.upgrade().ok_or(MailContextError::LostContext)?;
        let r = Save::apply_remote_impl(&ctx, action, &mut guard).await;

        if let Err(e) = &r
            && let Err(e) = save_send_error(action, &mut guard, e).await
        {
            error!("Failed to save draft send result: {e:?}");
        }

        r
    }
}

impl Save {
    async fn apply_remote_impl(
        ctx: &MailUserContext,
        action: &mut Self,
        guard: &mut WriterGuard<'_>,
    ) -> Result<<Self as Action>::RemoteOutput, <Self as Action>::Error> {
        let session = ctx.session();

        let local_message_id = action.message_id.expect("Should be set");
        let conversation_id = action.conversation_id.expect("Should be set");

        if Message::find_by_id(local_message_id, guard.tether())
            .await?
            .is_none()
        {
            error!("Message {local_message_id} does not exits");
            return Err(AppError::MessageMissing(local_message_id).into());
        };

        if Conversation::find_by_id(conversation_id, guard.tether())
            .await?
            .is_none()
        {
            return Err(AppError::ConversationNotFound(conversation_id).into());
        };

        let remote_parent_id = if let Some(parent_id) = action.parent_id {
            let Some(remote_id) = Message::local_id_counterpart(parent_id, guard.tether())
                .await
                .inspect_err(|e| error!("Failed to resolve remote parent id: {e:?}"))?
            else {
                error!("Could not find parent message with id {parent_id:?}");
                return Err(AppError::MessageMissing(parent_id).into());
            };

            Some(remote_id)
        } else {
            None
        };

        let draft_reply_or_forward_params = if let (Some(remote_parent_id), Some(reply_mode)) =
            (remote_parent_id, action.reply_mode)
        {
            Some(DraftReplyOrForwardParams {
                parent_id: remote_parent_id,
                action: reply_mode.into(),
            })
        } else {
            None
        };

        // Resolve remote ids. We captured the state of the message and the body metadata, but
        // other actions could have run at this point in time which may have updated the remote
        // ids.

        // Check message id.
        let remote_message_id = Message::local_id_counterpart(local_message_id, guard.tether())
            .await
            .inspect_err(|e| error!("Failed to resolve remote message id: {e}"))?;

        // Reload attachments if they don't have remote id or key packets.
        let mut attachments =
            Attachment::find_by_ids(action.attachment_ids.iter().cloned(), guard.tether())
                .await
                .inspect_err(|e| error!("Failed to load attachments: {e:?}"))?
                .into_iter()
                .filter(|a| {
                    if a.remote_id().is_none() {
                        // When adding new attachment to a draft, we reflect the state correctly offline
                        // but we can not attach an attachment until it has a remote id. We skip attachments
                        // that still does not have a remote id. Since we always save before send and send
                        // also requires all attachments to be uploaded this will correct itself.
                        tracing::debug!(
                            "Attachment {} does not have a remote id, skipping",
                            a.local_id.unwrap()
                        );
                        false
                    } else {
                        true
                    }
                })
                .inspect(|a| {
                    tracing::debug!(
                        "With {:?}:{:?}",
                        a.id(),
                        a.remote_id().expect("Should be set")
                    )
                })
                .collect::<Vec<_>>();

        // Create draft on the server.
        let new_message = if let Some(remote_message_id) = remote_message_id.clone() {
            info!("Updating draft {:?}", remote_message_id);
            let result = Draft::remote_update(
                ctx,
                session,
                action.address_id.clone(),
                local_message_id,
                remote_message_id.clone(),
                action,
                &attachments,
                &action.body,
                guard.tether(),
            )
            .await
            .inspect_err(|e| {
                error!("Failed to update draft on remote: {e:?}");
            });
            if matches!(
                &result,
                Err(MailContextError::Draft(draft::Error::Save(
                    SaveError::AlreadySent
                ))) | Err(MailContextError::Draft(draft::Error::Save(
                    SaveError::MessageNotADraft(_)
                )))
            ) {
                // if we hit an already sent error, we should delete the draft metadata
                // move the message to sent and schedule a resync.
                info!("Draft is already sent, moving to sent folder");

                if let Err(e) = guard
                    .tx::<_, _, MailContextError>(async |tx| {
                        DraftMetadata::delete_by_id(action.metadata_id, tx).await?;

                        let local_draft_label_id = local_draft_label_id(tx).await?;
                        let local_sent_id = local_sent_label_id(tx).await?;
                        let local_all_draft_label_id = local_all_draft_label_id(tx).await?;

                        Message::remove_label_async(local_draft_label_id, [local_message_id], tx)
                            .await?;
                        Message::remove_label_async(
                            local_all_draft_label_id,
                            [local_message_id],
                            tx,
                        )
                        .await?;
                        Message::apply_label_async(local_sent_id, [local_message_id], tx).await?;

                        let mut rollback_item = RollbackItem::new(
                            remote_message_id.to_string(),
                            RollbackItemType::Message,
                        );
                        Ok(rollback_item.save(tx).await?)
                    })
                    .await
                {
                    // We should report the original error, but there is not much we can do
                    // if the transaction fails. The user will have to try again later.
                    error!("Failed to recover after draft already sent error: {e:?}");
                }

                return Err(MailContextError::Draft(draft::Error::Save(
                    SaveError::AlreadySent,
                )));
            }
            result?
        } else {
            info!("Creating new draft");
            let message = Draft::remote_create(
                ctx,
                session,
                action.address_id.clone(),
                action,
                &attachments,
                &action.body,
                draft_reply_or_forward_params,
                guard.tether(),
            )
            .await
            .inspect_err(|e| {
                error!("Failed to create draft on remote: {e:?}");
            })?;
            info!("Draft created with {:?}", message.metadata.id);
            message
        };

        // Note: This section will be generalized as part of ET-1353 when
        // we implement draft updates.
        guard
            .tx::<_, _, <Self as Action>::Error>(async |bond| {
                // check if someone else already created this conversation through some other
                // flow.
                if let Some(remote_conv_local_id) = Conversation::remote_id_counterpart(new_message.metadata.conversation_id.clone(), bond).await?
                    && remote_conv_local_id != conversation_id {
                        warn!("Draft conversation was synced by other means, patching data to preserve local changes");
                        // Someone else managed to create this before us, but we don't want this
                        // conversation to overwrite our local data so we remove it. This is safe
                        // to do since this only happens when we create a new empty draft. Replies
                        // and forwarding already have a conversation.

                        // Update message conversation id, it is possible that this was patched
                        // to the newly created conversation.
                        bond.execute(
                            "UPDATE messages SET local_conversation_id=? WHERE local_id=?",
                            params![conversation_id, local_message_id],
                        ).await?;

                        // Delete the other conversation.
                        Conversation::delete_by_id(remote_conv_local_id, bond).await?;
                    }
                    // Update the remote conversation id.
                    Conversation::update_remote_id(
                        conversation_id,
                        new_message.metadata.conversation_id.clone(),
                        bond,
                    )
                        .await
                        .inspect_err(|e| error!("Failed to update the conversation remote id: {e:?}"))?;

                // Update message data
                let (new_local_message, mut new_message_body_metadata, _) =
                    Message::from_api_data(new_message, bond)
                        .await
                        .inspect_err(|e| {
                            error!("Failed to convert api message: {e:?}");
                        })?;

                // Do not override all the data as it may override local data that we modified
                // but is out of date when we are making this request. The only value we should
                // care about is the display order. Everything else we control.
                Message::update_ids_and_display_order(
                    local_message_id,
                    new_local_message.display_order,
                    new_local_message.remote_id.expect("Should be set after api fetc"),
                    new_local_message.remote_conversation_id.expect("Should be set after api fetch"),
                    bond
                )
                    .await
                    .inspect_err(|e| error!("Failed to update the message: {e:?}"))?;

                if remote_message_id.is_none() {
                    // When we create a draft on the server, all inherited attachments get a new remote
                    // id. We need to remove and update those items for things to work correctly

                    // API always returns the same amount, but tests may not.
                    debug_assert_eq!(
                        new_message_body_metadata.attachments.len(),
                        attachments.iter().filter_map(|a| a.remote_id()).count()
                    );

                    // This is safe to do as API guarantees that inherited attachments from
                    // reply forward are updated in place. This is expected to happen only once.
                    for (index, original_attachment) in attachments
                        .iter_mut()
                        .enumerate()
                    {
                        let Some(remote_id) = &original_attachment.remote_id() else {
                            // We can't do this if the attachment has no remote id.
                            continue;
                        };
                        let Some(attachment_metadata) = DraftAttachmentMetadata::find_by_id(
                            original_attachment.id(),
                            bond,
                        )
                            .await?
                        else {
                            warn!(
                            "Could not find attachment with id {}",
                            original_attachment.id()
                        );
                            continue;
                        };

                        if attachment_metadata.ownership == DraftAttachmentOwnership::Inherited {
                            let new_attachment = &mut new_message_body_metadata.attachments[index];
                            debug_assert_eq!(original_attachment.filename, new_attachment.filename);
                            debug_assert_eq!(original_attachment.disposition, new_attachment.disposition);
                            debug_assert_eq!(original_attachment.content_id, new_attachment.content_id);
                            // Safe to unwrap, server responses always have remote id.
                            debug_assert_ne!(
                                original_attachment.remote_id().as_ref().unwrap(),
                                new_attachment.remote_id().as_ref().unwrap()
                            );
                            //Inherited attachment will be removed and replaced by a new id.
                            debug!(
                            "Removing inherited attachment {}: {}",
                            original_attachment.id(),
                            &remote_id,
                        );
                            // Unlink previous attachment.
                            bond.execute(indoc! {
                            "DELETE FROM message_attachments WHERE local_message_id=? AND local_attachment_id = ?",
                        }, params![local_message_id, original_attachment.id()]).await.inspect_err(|e|
                                error!("Failed to unlink attachment from message: {e:?}"))?;
                            // Remove attachment metadata
                            let current_display_order = attachment_metadata.display_order;
                            attachment_metadata.delete(bond).await.inspect_err(|e| {
                                error!("Failed to delete draft attachment metadata {e:?}")
                            })?;

                            // Create new attachment.
                            new_attachment
                                .save(bond)
                                .await
                                .inspect_err(|e| error!("Failed to save attachment: {e}"))?;
                            // Create link
                            bond.execute("INSERT INTO message_attachments (local_message_id, local_attachment_id) VALUES (?,?)", params![local_message_id, new_attachment.id()]).await.inspect_err(|e| error!("Failed to link new attachment: {e:?}"))?;
                            // Creat new metadata entry
                            let mut new_attachment_metadata = DraftAttachmentMetadata::owned_and_uploaded(
                                action.metadata_id,
                                new_attachment.id(),
                                current_display_order,
                                original_attachment.is_public_key_attachment(),
                            );
                            new_attachment_metadata.save(bond).await.inspect_err(|e| {
                                error!("Failed to save new draft attachment metadata: {e:?}")
                            })?;

                            // Ensure the newly created attachment has a data copy. This is required
                            // for sending to external (non-proton) addresses. However, it is possible
                            // the attachment has not been synced, so we can only do this if we have the
                            // data.
                            let original_attachment_id = original_attachment.id();
                            if let Some(path) = Attachment::path_from_cache_and_update_metadata(
                                original_attachment_id,
                                bond,
                            )
                                .await?
                            {
                                debug!("Attachment present in cache, performing copy");
                                Attachment::copy_attachment_to_cache(
                                    ctx,
                                    &new_attachment.filename,
                                    new_attachment.id(),
                                    &path,
                                    bond,
                                )
                                    .await?;
                            }

                            // Update the original attachment to make sure the next
                            // check is accurate and to avoid duplicate updates.
                            *original_attachment = new_attachment.clone()
                        }
                    }
                }

                // If address changed saving the attachments are upload with new key packets
                // which will reset their data. We need to check if this occurred here and
                // reset the signatures and update the key packets so that send works correctly.
                // Ordering of the attachments is only guaranteed if there are inherited attachments
                // from reply/forwarding. This only happens on time, the rest of the case
                // the order will not be guaranteed anymore.
                for original_attachment in attachments {
                    if let Some(new_attachment)= new_message_body_metadata.attachments.iter_mut().find(|new_attachment| {
                        original_attachment.remote_id() == new_attachment.remote_id() && (original_attachment.remote_address_id != new_attachment.remote_address_id || original_attachment.key_packets!= new_attachment.key_packets)
                    }) {
                        tracing::info!("Detected address change on attachment ({}/{})", original_attachment.local_id.unwrap(), original_attachment.remote_id().as_ref().unwrap());
                        new_attachment.local_id = original_attachment.local_id;
                        new_attachment.attachment_type = original_attachment.attachment_type.clone();
                        new_attachment.update_after_draft_address_change(bond).await?;
                    }
                }

                // Update only the headers that are produced by the api.
                new_message_body_metadata.local_message_id = Some(local_message_id);
                new_message_body_metadata
                    .update_fields_after_draft_create_or_update(bond)
                    .await
                    .inspect_err(|e| {
                        error!("Failed to update message body metadata: {e:?}");
                    })?;
                Ok(())
            }).await
    }

    #[allow(clippy::too_many_arguments)]
    fn create_new_message(
        &self,
        address: &Address,
        sender_email: String,
        total_attachment_count: u64,
        attachments: Vec<AttachmentMetadata>,
        body_len: u64,
        time: UnixTimestamp,
        display_order: u64,
    ) -> Message {
        debug_assert!(
            attachments
                .iter()
                .all(|v| v.disposition == Disposition::Attachment)
        );
        Message {
            local_id: None,
            remote_id: None,
            local_conversation_id: None,
            remote_conversation_id: None,
            local_address_id: address.id(),
            remote_address_id: address.remote_id.clone().unwrap(),
            attachments_metadata: attachments,
            cc_list: self.cc_list.to_message_recipients().into(),
            bcc_list: self.bcc_list.to_message_recipients().into(),
            deleted: false,
            exclusive_location: None,
            expiration_time: 0.into(),
            external_id: None,
            flags: Default::default(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![],
            num_attachments: total_attachment_count.try_into().unwrap_or_default(),
            display_order,
            sender: MessageSender {
                address: sender_email.into(),
                name: address.display_name.clone().into(),
                ..Default::default()
            },
            size: body_len,
            snooze_time: Default::default(),
            subject: self.subject.clone(),
            time,
            to_list: self.to_list.to_message_recipients().into(),
            unread: false,
            custom_labels: vec![],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_message(
        &self,
        address: &Address,
        sender_email: String,
        message: &mut Message,
        total_attachment_count: u64,
        attachments: Vec<AttachmentMetadata>,
        body_len: u64,
        time: UnixTimestamp,
    ) {
        message.local_address_id = address.id();
        message.remote_address_id = address.remote_id.clone().unwrap();
        message.attachments_metadata = attachments;
        message.to_list = self.to_list.to_message_recipients().into();
        message.cc_list = self.cc_list.to_message_recipients().into();
        message.bcc_list = self.bcc_list.to_message_recipients().into();
        message.num_attachments = total_attachment_count.try_into().unwrap_or_default();
        message.sender = MessageSender {
            address: sender_email.clone().into(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: address.display_name.clone().into(),
        };
        message.size = body_len;
        message.subject = self.subject.clone();
        message.time = time;
        message.expiration_time = 0.into();
    }

    #[allow(clippy::too_many_arguments)]
    fn create_new_conversation(
        &self,
        address: &Address,
        sender_email: String,
        display_order: u64,
        body_len: u64,
        attachments: Vec<AttachmentMetadata>,
        total_attachment_count: u64,
        subject: String,
    ) -> Conversation {
        debug_assert!(
            attachments
                .iter()
                .all(|v| v.disposition == Disposition::Attachment)
        );
        Conversation {
            local_id: None,
            remote_id: None,
            attachment_info: Default::default(),
            attachments_metadata: attachments,
            deleted: false,
            display_snooze_reminder: false,
            exclusive_location: None,
            expiration_time: 0.into(),
            labels: vec![],
            num_attachments: total_attachment_count,
            num_messages: 0,
            num_unread: 0,
            display_order,
            recipients: Default::default(),
            senders: MessageSenders {
                value: vec![MessageSender {
                    address: sender_email.clone().into(),
                    bimi_selector: None,
                    display_sender_image: false,
                    is_proton: true,
                    is_simple_login: false,
                    name: address.display_name.clone().into(),
                }],
            },
            size: body_len,
            subject,
            is_known: false,
            custom_labels: vec![],
            has_messages: true,
            snoozed_until: None,
        }
    }

    async fn attachments(&self, tether: &Bond<'_>) -> Result<Vec<Attachment>, StashError> {
        DraftAttachmentMetadata::attachment_for_draft(self.metadata_id, tether).await
    }
    fn attachment_metadata(attachments: &[Attachment]) -> Vec<AttachmentMetadata> {
        attachments
            .iter()
            .filter(|attachment| attachment.disposition == Disposition::Attachment)
            .map(|attachment| AttachmentMetadata::from(attachment.clone()))
            .collect()
    }
}

// Simple wrapper function to catch errors
async fn save_send_error(
    action: &Save,
    guard: &mut WriterGuard<'_>,
    error: &MailContextError,
) -> Result<(), WriterGuardError> {
    let error = DraftSendFailure::from_mail_context_error(error);
    if error.is_skippable() {
        return Ok(());
    }
    let mut send_result = DraftSendResult::failure(
        action.message_id.expect("Should be set by now"),
        action.save_origin,
        error,
    );
    guard
        .tx::<_, _, WriterGuardError>(async |tx| Ok(send_result.save(tx).await?))
        .await?;
    Ok(())
}
