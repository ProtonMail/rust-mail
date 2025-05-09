use crate::actions::draft::{
    SEND_ACTION_GROUP, local_all_draft_label_id, local_all_mail_label_id, local_draft_label_id,
};
use crate::datatypes::{
    AttachmentMetadata, Disposition, LocalMessageId, MessageSender, MessageSenders, MimeType,
    SystemLabelId,
};
use crate::draft::compose::maybe_sanitize;
use crate::draft::recipients::RecipientList;
use crate::draft::{Draft, ReplyMode, SaveError, compose};
use crate::models::{
    Attachment, Conversation, DraftAttachmentMetadata, DraftAttachmentOwnership, DraftMetadata,
    DraftSendFailure, DraftSendResult, DraftSendResultOrigin, Message, MessageBodyMetadata,
    MetadataId,
};
use crate::{AppError, MailContextError, MailUserContext, draft};
use indoc::{formatdoc, indoc};
use proton_action_queue::action::{
    Action, ActionGroup, ActionId, DefaultVersionConverter, Priority, Type, WriterGuard,
    WriterGuardError,
};
use proton_api_core::services::proton::{AddressId, LabelId};
use proton_api_mail::services::proton::prelude::{
    DraftParams, DraftReplyOrForwardParams, ExternalId,
};
use proton_api_mail::services::proton::request_data::DraftSender;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_crypto_inbox::message::EncryptedDraft;
use proton_mail_ids::{LocalAttachmentId, LocalConversationId};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError};
use tracing::{debug, error, warn};

/// Action which creates or updates a draft on the server.
///
/// When the draft is successfully created, the remote ids for
/// the conversation and message are updated.
///
/// If the action failed, nothing is reverted.
#[derive(Serialize, Deserialize, Clone)]
pub struct Save {
    metadata_id: MetadataId,
    /// To Recipients - only email to preserve display name privacy
    to_list: RecipientList,
    /// CC Recipients - only email to preserve display name privacy
    cc_list: RecipientList,
    /// BCC recipients - only email to preserve display name privacy
    bcc_list: RecipientList,
    /// Local id of the message this conversation belongs to
    message_id: Option<LocalMessageId>,
    /// Local id of the conversation this message belongs to
    conversation_id: Option<LocalConversationId>,
    /// Address used to send the message
    address_id: AddressId,
    /// Draft subject
    subject: String,
    /// Unencrypted body of the draft
    body: String,
    /// Draft's mime type
    mime_type: MimeType,
    /// Parent message id, used with forward and update only.
    parent_id: Option<LocalMessageId>,
    /// Reply mode used.
    reply_mode: Option<ReplyMode>,
    /// For error reporting when action fails
    save_origin: DraftSendResultOrigin,
    /// Attachments associated with this message.
    attachment_ids: Vec<LocalAttachmentId>,
    /// Message's external id.
    external_id: Option<ExternalId>,
    /// Whether the draft is unread or not.
    unread: Option<bool>,
    /// Draft Sender
    sender: Option<MessageSender>,
}

impl Save {
    /// Create a new empty draft.
    pub fn new(draft: &Draft, save_origin: DraftSendResultOrigin) -> Self {
        // Undo all transformations to the body
        let transformed = maybe_sanitize(draft.mime_type(), draft.body().to_owned());
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
        }
    }

    /// Convert the existing action state into a [`DraftParams`].
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
            mime_type: self.mime_type.into(),
        }
    }
}

impl Action for Save {
    const TYPE: Type = Type("save_draft");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::High;
    const GROUP: ActionGroup = SEND_ACTION_GROUP;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = SaveHandler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailContextError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct SaveHandler {}

impl proton_action_queue::action::Handler for SaveHandler {
    type Action = Save;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        action_id: ActionId,
        _: &MailUserContext,
        action: &mut Self::Action,
        bond: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
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

        let body_len = action.body.len() as u64;
        let Some(address) = Address::find_by_remote_id(action.address_id.clone(), bond)
            .await
            .inspect_err(|e| error!("Failed to load address: {e:?}"))?
        else {
            error!("Address with remote id {:?} not found.", action.address_id);
            return Err(SaveError::AddressNotFound(action.address_id.clone()).into());
        };

        let attachments = action
            .attachments(bond)
            .await
            .inspect_err(|e| error!("Failed to load attachments: {e:?}"))?;
        debug!("Draft has {} attachments", attachments.len());
        let attachment_metadata = Save::attachment_metadata(&attachments);
        let attachment_ids = attachments
            .iter()
            .map(|a| a.local_id.unwrap())
            .collect::<Vec<_>>();

        let conversation_id = if let Some(id) = metadata.local_conversation_id {
            id
        } else {
            debug!("Conversation does not exist, creating");
            let display_order = Conversation::next_display_order(bond)
                .await
                .inspect_err(|e| error!("Failed to get next conversation display order: {e:?}"))?;
            let mut conversation = action.create_new_conversation(
                &address,
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
            metadata.local_conversation_id = Some(conversation.local_id.unwrap());
            conversation.local_id.unwrap()
        };

        let time = draft::compose::create_timestamp();
        let message = if let Some(message_id) = metadata.local_message_id {
            debug!("Local message id is set, update");
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
            body_metadata.mime_type = action.mime_type;

            body_metadata.save(bond).await.inspect_err(|e| {
                error!("Failed to update draft body metadata: {e:?}");
            })?;

            message
        } else {
            debug!("Local message id is not set, creating new draft");
            let display_order = Message::next_display_order(bond)
                .await
                .inspect_err(|e| error!("Failed to get next message display order: {e:?}"))?;
            let mut message = action.create_new_message(
                &address,
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
                local_message_id: Some(message.local_id.unwrap()),
                remote_message_id: None,
                header: "".to_string(),
                mime_type: action.mime_type,
                parsed_headers: Default::default(),
                attachments,
                row_id: None,
            };

            message_body_metadata
                .save(bond)
                .await
                .inspect_err(|e| error!("Failed to save message body metadata: {e:?}"))?;

            Message::apply_label(
                local_draft_id,
                std::iter::once(message.local_id.unwrap()),
                bond,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to apply draft label to new message: {e:?}");
            })?;

            Message::apply_label(
                local_all_draft_id,
                std::iter::once(message.local_id.unwrap()),
                bond,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to apply all_draft label to new message: {e:?}");
            })?;

            Message::apply_label(
                local_all_mail_id,
                std::iter::once(message.local_id.unwrap()),
                bond,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to apply all_mail label to new message: {e:?}");
            })?;

            message
        };

        Message::store_decrypted_message_body(message.local_id.unwrap(), action.body.clone(), bond)
            .await
            .inspect_err(|e| {
                error!("Failed to store draft body in cache :{e:?}");
            })?;

        metadata.local_message_id = Some(message.local_id.unwrap());
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
        _: &MailUserContext,
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
        ctx: &MailUserContext,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let r = Save::apply_remote_impl(ctx, action, &mut guard).await;
        if let Err(e) = &r {
            if let Err(e) = save_send_error(action, &mut guard, e).await {
                error!("Failed to save draft send result: {e:?}");
            }
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
        debug!("Resolving remote message id");
        let remote_message_id = Message::local_id_counterpart(local_message_id, guard.tether())
            .await
            .inspect_err(|e| error!("Failed to resolve remote message id: {e}"))?;
        if remote_message_id.is_none() {
            debug!("Message does not have remote id yet");
        }

        // Reload attachments if they don't have remote id or key packets.
        let attachments =
            Attachment::find_by_ids(action.attachment_ids.iter().cloned(), guard.tether())
                .await
                .inspect_err(|e| error!("Failed to load attachments: {e:?}"))?;

        // Create draft on the server.
        let new_message = if let Some(remote_message_id) = remote_message_id.clone() {
            Draft::remote_update(
                ctx,
                session,
                action.address_id.clone(),
                local_message_id,
                remote_message_id,
                action,
                &attachments,
                &action.body,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to update draft on remote: {e:?}");
            })?
        } else {
            Draft::remote_create(
                ctx,
                session,
                action.address_id.clone(),
                action,
                &attachments,
                &action.body,
                draft_reply_or_forward_params,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to create draft on remote: {e:?}");
            })?
        };

        // Note: This section will be generalized as part of ET-1353 when
        // we implement draft updates.
        guard
        .tx::<_, _, <Self as Action>::Error>(async |bond| {
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

            if remote_message_id.is_none() {
                // When we create a draft on the server, all inherited attachments get a new remote
                // id. We need to remove and update those items for things to work correctly

                // API always returns the same amount, but tests may not.
                debug_assert_eq!(
                    new_message_body_metadata.attachments.len(),
                    attachments.iter().filter_map(|a| a.remote_id()).count()
                );

                for (index, original_attachment) in attachments
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.remote_id().is_some())
                {
                    let Some(remote_id) = &original_attachment.remote_id() else {
                        // We can't do this if the attachment has no remote id.
                        continue;
                    };
                    let Some(attachment_metadata) = DraftAttachmentMetadata::find_by_id(
                        original_attachment.local_id.unwrap(),
                        bond,
                    )
                        .await?
                    else {
                        warn!(
                            "Could not find attachment with id {}",
                            original_attachment.local_id.unwrap()
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
                            original_attachment.local_id.unwrap(),
                            &remote_id,
                        );
                        // Unlink previous attachment.
                        bond.execute(indoc! {
                            "DELETE FROM message_attachments WHERE local_message_id=? AND local_attachment_id = ?",
                        }, params![local_message_id, original_attachment.local_id.unwrap()]).await.inspect_err(|e|
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
                        bond.execute("INSERT INTO message_attachments (local_message_id, local_attachment_id) VALUES (?,?)", params![local_message_id, new_attachment.local_id.unwrap()]).await.inspect_err(|e| error!("Failed to link new attachment: {e:?}"))?;
                        // Creat new metadata entry
                        let mut new_attachment_metadata = DraftAttachmentMetadata::owned_and_uploaded(
                            action.metadata_id,
                            new_attachment.local_id.unwrap(),
                            current_display_order,
                        );
                        new_attachment_metadata.save(bond).await.inspect_err(|e| {
                            error!("Failed to save new draft attachment metadata: {e:?}")
                        })?;

                        // Ensure the newly created attachment has a data copy. This is required
                        // for sending to external (non-proton) addresses. However, it is possible
                        // the attachment has not been synced, so we can only do this if we have the
                        // data.
                        let original_attachment_id = original_attachment.local_id.unwrap();
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
                                new_attachment.local_id.unwrap(),
                                &path,
                                bond,
                            )
                                .await?;
                        }
                    }
                }
            }

            // Do not override all the data as it may override local data that we modified
            // but is out of date when we are making this request. The only value we should
            // care about is the display order. Everything else we control.
            bond.execute(
                formatdoc! {"
            UPDATE {} SET
                display_order = ?,
                remote_id =?,
                remote_conversation_id =?
            WHERE local_id = ?
        ", Message::table_name()},
                params![
                    new_local_message.display_order,
                    new_local_message.remote_id,
                    new_local_message.remote_conversation_id,
                    local_message_id
                ],
            )
                .await
                .inspect_err(|e| error!("Failed to update the message: {e:?}"))?;

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
    fn create_new_message(
        &self,
        address: &Address,
        total_attachment_count: u64,
        attachments: Vec<AttachmentMetadata>,
        body_len: u64,
        time: u64,
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
            local_address_id: address.local_id.unwrap(),
            remote_address_id: address.remote_id.clone().unwrap(),
            attachments_metadata: attachments,
            cc_list: self.cc_list.to_message_recipients().into(),
            bcc_list: self.bcc_list.to_message_recipients().into(),
            deleted: false,
            exclusive_location: None,
            expiration_time: 0,
            external_id: None,
            flags: Default::default(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![],
            num_attachments: total_attachment_count.try_into().unwrap_or_default(),
            display_order,
            reply_tos: Default::default(),
            sender: MessageSender {
                address: address.email.clone(),
                bimi_selector: None,
                display_sender_image: false,
                is_proton: false,
                is_simple_login: false,
                name: address.display_name.clone(),
            },
            size: body_len,
            snooze_time: 0,
            subject: self.subject.clone(),
            time,
            to_list: self.to_list.to_message_recipients().into(),
            unread: false,
            custom_labels: vec![],
            row_id: None,
        }
    }

    fn update_message(
        &self,
        address: &Address,
        message: &mut Message,
        total_attachment_count: u64,
        attachments: Vec<AttachmentMetadata>,
        body_len: u64,
        time: u64,
    ) {
        message.local_address_id = address.local_id.unwrap();
        message.remote_address_id = address.remote_id.clone().unwrap();
        message.attachments_metadata = attachments;
        message.to_list = self.to_list.to_message_recipients().into();
        message.cc_list = self.cc_list.to_message_recipients().into();
        message.bcc_list = self.bcc_list.to_message_recipients().into();
        message.num_attachments = total_attachment_count.try_into().unwrap_or_default();
        message.sender = MessageSender {
            address: address.email.clone(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: address.display_name.clone(),
        };
        message.size = body_len;
        message.subject = self.subject.clone();
        message.time = time;
    }

    fn create_new_conversation(
        &self,
        address: &Address,
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
            expiration_time: 0,
            labels: vec![],
            num_attachments: total_attachment_count,
            num_messages: 0,
            num_unread: 0,
            display_order,
            recipients: Default::default(),
            senders: MessageSenders {
                value: vec![MessageSender {
                    address: address.email.clone(),
                    is_proton: true,
                    ..Default::default()
                }],
            },
            size: body_len,
            subject,
            is_known: false,
            custom_labels: vec![],
            has_messages: false,
            row_id: None,
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
