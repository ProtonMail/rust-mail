use crate::actions::draft::{
    local_all_draft_label_id, local_all_mail_label_id, local_draft_label_id,
};
use crate::datatypes::{
    AttachmentMetadata, Disposition, LocalAttachmentId, LocalMessageId, MessageSender,
    MessageSenders, MimeType, SystemLabelId,
};
use crate::decrypted_message::StorableMessageBodyRef;
use crate::draft::recipients::RecipientList;
use crate::draft::{compose, Draft, ReplyMode, SaveOrSendError};
use crate::models::{
    Attachment, Conversation, DraftMetadata, DraftSendFailure, DraftSendResult,
    DraftSendResultOrigin, Message, MessageBodyMetadata, MetadataId,
};
use crate::{draft, AppError, MailContextError, MailUserContext};
use proton_action_queue::action::{Action, DefaultVersionConverter, Priority, Type};
use proton_api_core::services::proton::common::{AddressId, LabelId};
use proton_api_mail::services::proton::prelude::DraftReplyOrForwardParams;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_mail_ids::LocalConversationId;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError};
use tracing::{debug, error};

/// Action which creates or updates a draft on the server.
///
/// When the draft is successfully created, the remote ids for
/// the conversation and message are updated.
///
/// If the action failed, nothing is reverted.
#[derive(Serialize, Deserialize)]
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
    ///
    /// This is only used when creating local state and is not needed
    /// afterwards.
    #[serde(skip)]
    body: String,
    /// Attachment associated with this draft
    attachments: Vec<LocalAttachmentId>,
    /// Draft's mime type
    mime_type: MimeType,
    /// Parent message id, used with forward and update only.
    parent_id: Option<LocalMessageId>,
    /// Reply mode used.
    reply_mode: Option<ReplyMode>,
    /// For error reporting when action fails
    save_origin: DraftSendResultOrigin,
}

impl Save {
    /// Create a new empty draft.
    pub fn new(draft: &Draft, save_origin: DraftSendResultOrigin) -> Self {
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
            body: draft.decrypted_body.body.clone(),
            attachments: draft
                .decrypted_body
                .metadata
                .attachments
                .iter()
                .map(|v| v.local_id.unwrap())
                .collect(),
            mime_type: draft.decrypted_body.metadata.mime_type,
            parent_id: None,
            reply_mode: None,
            save_origin,
        }
    }
}

impl Action for Save {
    const TYPE: Type = Type("save_draft");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Highest;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = WrappedSaveHandler;
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
        ctx: &MailUserContext,
        action: &mut Self::Action,
        tether: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let local_draft_id = local_draft_label_id(tether).await?;
        let local_all_draft_id = local_all_draft_label_id(tether).await?;
        let local_all_mail_id = local_all_mail_label_id(tether).await?;

        let Some(mut metadata) = DraftMetadata::find_by_id(action.metadata_id, tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e:?}");
            })?
        else {
            error!("Could not find metadata {:?}", action.metadata_id);
            return Err(SaveOrSendError::MetadataNotFound(action.metadata_id).into());
        };

        let body_len = action.body.len() as u64;
        let Some(address) = Address::find_by_remote_id(action.address_id.clone(), tether)
            .await
            .inspect_err(|e| error!("Failed to load address: {e:?}"))?
        else {
            error!("Address with remote id {:?} not found.", action.address_id);
            return Err(SaveOrSendError::AddressNotFound(action.address_id.clone()).into());
        };

        let attachments = action
            .attachments(tether)
            .await
            .inspect_err(|e| error!("Failed to load attachments: {e:?}"))?;
        let attachment_metadata = Save::attachment_metadata(&attachments);

        let conversation_id = if let Some(id) = metadata.local_conversation_id {
            id
        } else {
            debug!("Conversation does not exist, creating");
            let display_order = Conversation::next_display_order(tether)
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
                .save(tether)
                .await
                .inspect_err(|e| error!("Failed to create new conversation: {e:?}"))?;
            metadata.local_conversation_id = Some(conversation.local_id.unwrap());
            conversation.local_id.unwrap()
        };

        let time = draft::compose::create_timestamp();
        let message = if let Some(message_id) = metadata.local_message_id {
            debug!("Local message id is set, update");
            let Some(mut message) = Message::find_by_id(message_id, tether)
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
                return Err(SaveOrSendError::AlreadySent.into());
            }

            action.update_message(
                &address,
                &mut message,
                attachments.len() as u64,
                attachment_metadata,
                body_len,
                time,
            );

            message.save(tether).await.inspect_err(|e| {
                error!("Failed to update draft message: {e:?}");
            })?;

            let Some(mut body_metadata) = MessageBodyMetadata::for_message(message_id, tether)
                .await
                .inspect_err(|e| error!("Failed to load message metadata: {e:?}"))?
            else {
                return Err(AppError::MessageMissing(message_id).into());
            };

            body_metadata.attachments = attachments;
            body_metadata.mime_type = action.mime_type;

            body_metadata.save(tether).await.inspect_err(|e| {
                error!("Failed to update draft body metadata: {e:?}");
            })?;

            message
        } else {
            debug!("Local message id is not set, creating new draft");
            let display_order = Message::next_display_order(tether)
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
                .save(tether)
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
                .save(tether)
                .await
                .inspect_err(|e| error!("Failed to save message body metadata: {e:?}"))?;

            Message::apply_label(
                local_draft_id,
                std::iter::once(message.local_id.unwrap()),
                tether,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to apply draft label to new message: {e:?}");
            })?;

            Message::apply_label(
                local_all_draft_id,
                std::iter::once(message.local_id.unwrap()),
                tether,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to apply all_draft label to new message: {e:?}");
            })?;

            Message::apply_label(
                local_all_mail_id,
                std::iter::once(message.local_id.unwrap()),
                tether,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to apply all_mail label to new message: {e:?}");
            })?;

            message
        };

        // Store body in cache.
        let raw_body = StorableMessageBodyRef {
            body: &action.body,
            ..Default::default()
        };

        Message::store_raw_message_in_cache(ctx, message.local_id.unwrap(), raw_body).inspect_err(
            |e| {
                error!("Failed to store draft body in cache :{e:?}");
            },
        )?;

        metadata.local_message_id = Some(message.local_id.unwrap());
        metadata.save(tether).await.inspect_err(|e| {
            error!("Failed to save draft metadata: {e:?}");
        })?;

        action.message_id = metadata.local_message_id;
        action.conversation_id = metadata.local_conversation_id;
        action.reply_mode = metadata.reply_mode;
        action.parent_id = metadata.local_parent_id;

        Ok(())
    }

    async fn revert_local(
        &self,
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
        ctx: &MailUserContext,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let session = ctx.session();
        let mut tether = stash.connection();

        let message_id = action.message_id.expect("Should be set");
        let conversation_id = action.conversation_id.expect("Should be set");

        // Load all dependencies to make sure they are up to date. For drafts
        // this is fine so we can always access the latest value of the data
        // without having to queue multiple actions.
        let Some(mut message) = Message::find_by_id(message_id, &tether).await? else {
            return Err(AppError::MessageMissing(message_id).into());
        };

        if Conversation::find_by_id(conversation_id, &tether)
            .await?
            .is_none()
        {
            return Err(AppError::ConversationNotFound(conversation_id).into());
        };

        let Some(mut message_body_metadata) = MessageBodyMetadata::for_message(message_id, &tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load message body metadata for {message_id}: {e:?}")
            })?
        else {
            return Err(AppError::MessageBodyMetadataMissing(message_id).into());
        };

        let remote_parent_id = if let Some(parent_id) = action.parent_id {
            let Some(remote_id) = Message::local_id_counterpart(parent_id, &tether)
                .await
                .inspect_err(|e| error!("Failed to resolve remote parent id: {e:?}"))?
            else {
                error!("Could not find parent message with id {parent_id}");
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

        // Load body.
        let Some(message_body) =
            Message::load_decrypted_message_body_from_cache(ctx, message.local_id.unwrap())?
        else {
            return Err(AppError::MessageBodyMissing(message.local_id.unwrap()).into());
        };

        // Create draft on the server.
        let new_message = if message.remote_id.is_none() {
            Draft::remote_create(
                ctx,
                session,
                action.address_id.clone(),
                &message,
                &message_body_metadata,
                &message_body.body,
                draft_reply_or_forward_params,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to create draft on remote: {e:?}");
            })?
        } else {
            Draft::remote_update(
                ctx,
                session,
                action.address_id.clone(),
                &message,
                &message_body_metadata,
                &message_body.body,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to update draft on remote: {e:?}");
            })?
        };

        // Note: This section will be generalized as part of ET-1353 when
        // we implement draft updates.
        let bond = tether.transaction().await?;
        let row_id = message.row_id;

        // Update remote ids
        message.remote_id = Some(new_message.metadata.id.clone());
        message.remote_conversation_id = Some(new_message.metadata.conversation_id.clone());

        // Update the remote conversation id.
        Conversation::update_remote_id(
            conversation_id,
            new_message.metadata.conversation_id.clone(),
            &bond,
        )
        .await
        .inspect_err(|e| error!("Failed to update the conversation remote id: {e:?}"))?;

        // Because we can't have custom update function in stash we need to
        // first set the remote id on the message body metadata and then
        // we can save the metadata returned by the server.
        message_body_metadata.remote_message_id = message.remote_id.clone();
        message_body_metadata.save(&bond).await.inspect_err(|e| {
            error!("Failed to save message body metadata with remote id: {e:?}")
        })?;

        // Update message data
        let (mut new_local_message, mut new_message_body_metadata, _) =
            Message::from_api_data(new_message, &bond)
                .await
                .inspect_err(|e| {
                    error!("Failed to convert api message: {e:?}");
                })?;

        // Do not override all the data as it may override local data that we modified
        // but is out of date when we are making this request.
        new_local_message.subject = message.subject;
        new_local_message.to_list = message.to_list;
        new_local_message.cc_list = message.cc_list;
        new_local_message.bcc_list = message.bcc_list;
        new_local_message.sender = message.sender;
        new_local_message.num_attachments = message.num_attachments;
        new_local_message.attachments_metadata = message.attachments_metadata;

        new_local_message.row_id = row_id;
        new_local_message.local_id = Some(message_id);
        new_local_message.save(&bond).await.inspect_err(|e| {
            error!("Failed to update the message: {e:?}");
        })?;

        // Update body metadata
        new_message_body_metadata.local_message_id = Some(message_id);
        new_message_body_metadata.row_id = message_body_metadata.row_id;
        // Keep the existing attachment data as this may be overridden by the server
        // response if we have multiple saves.
        new_message_body_metadata.attachments = message_body_metadata.attachments;
        new_message_body_metadata
            .save(&bond)
            .await
            .inspect_err(|e| {
                error!("Failed to update message body metadata: {e:?}");
            })?;

        bond.commit().await?;

        Ok(())
    }
}

impl Save {
    fn create_new_message(
        &self,
        address: &Address,
        total_attachment_count: u64,
        attachments: Vec<AttachmentMetadata>,
        body_len: u64,
        time: u64,
        display_order: u64,
    ) -> Message {
        debug_assert!(attachments
            .iter()
            .all(|v| v.disposition == Disposition::Attachment));
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
        debug_assert!(attachments
            .iter()
            .all(|v| v.disposition == Disposition::Attachment));
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
        Attachment::find_by_ids(self.attachments.iter().cloned(), tether).await
    }
    fn attachment_metadata(attachments: &[Attachment]) -> Vec<AttachmentMetadata> {
        attachments
            .iter()
            .filter(|attachment| attachment.disposition == Disposition::Attachment)
            .map(|attachment| AttachmentMetadata::from(attachment.clone()))
            .collect()
    }
}

/// Wraps the execution of the default draft [`SaveHandler`] to record
/// remote execution failure.
#[derive(Default)]
pub struct WrappedSaveHandler(SaveHandler);

impl proton_action_queue::action::Handler for WrappedSaveHandler {
    type Action = <SaveHandler as proton_action_queue::action::Handler>::Action;
    type Context = <SaveHandler as proton_action_queue::action::Handler>::Context;

    async fn apply_local(
        &self,
        context: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        self.0.apply_local(context, action, tx).await
    }

    async fn revert_local(
        &self,
        context: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        self.0.revert_local(context, action, tx).await
    }

    async fn apply_remote(
        &self,
        context: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let r = self.0.apply_remote(context, action, stash).await;
        if let Err(e) = &r {
            if let Err(e) = save_send_error(action, stash, e).await {
                error!("Failed to save draft send result: {e:?}");
            }
        }
        r
    }
}

// Simple wrapper function to catch errors
async fn save_send_error(
    action: &Save,
    stash: &Stash,
    error: &MailContextError,
) -> Result<(), StashError> {
    let error = DraftSendFailure::from_mail_context_error(error);
    if error.is_skippable() {
        return Ok(());
    }
    let mut send_result = DraftSendResult::failure(
        action.message_id.expect("Should be set by now"),
        action.save_origin,
        error,
    );
    let mut conn = stash.connection();
    let tx = conn.transaction().await?;
    send_result.save(&tx).await?;
    tx.commit().await
}
