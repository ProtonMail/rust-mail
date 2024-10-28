use crate::cache::{CacheMessageConfig, CacheMessageKey};
use crate::datatypes::{
    AttachmentMetadata, Disposition, MessageAddress, MessageAddresses, MimeType, SystemLabelId,
};
use crate::draft::{Draft, Error, ReplyMode};
use crate::models::{
    Attachment, Conversation, DraftMetadata, Label, Message, MessageBodyMetadata, MetadataId,
};
use crate::{draft, AppError, MailContextError, MailUserContext};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::Session;
use proton_api_mail::services::proton::request_data::DraftAction;
use proton_core_common::cache::ProtonCache;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use proton_core_common::models::{Address, ModelExtension};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError, Tether};
use std::io::Read;
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
    /// Sender email
    sender: String,
    /// To Recipients - only email to preserve display name privacy
    to_list: Vec<String>,
    /// CC Recipients - only email to preserve display name privacy
    cc_list: Vec<String>,
    /// BCC recipients - only email to preserve display name privacy
    bcc_list: Vec<String>,
    /// Local id of the message this conversation belongs to
    message_id: Option<LocalId>,
    /// Local id of the conversation this message belongs to
    conversation_id: Option<LocalId>,
    /// Address used to send the message
    address_id: RemoteId,
    /// Draft subject
    subject: String,
    /// Unencrypted body of the draft
    ///
    /// This is only used when creating local state and is not needed
    /// afterwards.
    #[serde(skip)]
    body: String,
    /// Attachment associated with this draft
    attachments: Vec<LocalId>,
    /// Draft's mime type
    mime_type: MimeType,
    /// Parent message id, used with forward and update only.
    parent_id: Option<LocalId>,
    /// Reply mode used.
    reply_mode: Option<ReplyMode>,
    /// Whether to create or update the message.
    created_message: bool,
    /// Whether the conversation was created - used for cleanup
    conversation_created: bool,
}

impl Save {
    /// Create a new empty draft.
    pub fn new(draft: &Draft) -> Self {
        Self {
            metadata_id: draft.metadata_id,
            sender: draft.sender.clone(),
            to_list: draft.to_list.clone(),
            cc_list: draft.cc_list.clone(),
            bcc_list: draft.bcc_list.clone(),
            message_id: None,
            conversation_id: None,
            address_id: draft.address_id.clone(),
            subject: draft.subject.clone(),
            body: draft.body.clone(),
            attachments: draft
                .attachments
                .iter()
                .map(|v| v.local_id.unwrap())
                .collect(),
            mime_type: draft.mime_type,
            parent_id: None,
            reply_mode: None,
            created_message: false,
            conversation_created: false,
        }
    }
}

impl Action for Save {
    const TYPE: Type = Type("save_draft");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailContextError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Save;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        ctx: &MailUserContext,
        action: &mut Self::Action,
        tether: &Tether,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        let local_draft_id = local_draft_label_id(tether).await?;

        let Some(mut metadata) = DraftMetadata::find_by_id(action.metadata_id, tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load draft metadata: {e}");
            })?
        else {
            error!("Could not find metadata {}", action.metadata_id);
            return Err(Error::MetadataNotFound(action.metadata_id).into());
        };

        let body_len = action.body.len() as u64;
        let Some(address) = Address::find_by_id(action.address_id.clone(), tether)
            .await
            .inspect_err(|e| error!("Failed to load address: {e}"))?
        else {
            error!("Address with remote id {} not found.", action.address_id);
            return Err(Error::AddressNotFound(action.address_id.clone()).into());
        };

        let mut created_conversation = false;
        let mut created_message = false;

        let attachments = action
            .attachments(tether)
            .await
            .inspect_err(|e| error!("Failed to load attachments: {e}"))?;
        let attachment_metadata = Save::attachment_metadata(&attachments);

        let conversation_id = if let Some(id) = metadata.local_conversation_id {
            id
        } else {
            debug!("Conversation does not exist, creating");
            let display_order = Conversation::next_display_order(tether)
                .await
                .inspect_err(|e| error!("Failed to get next conversation display order: {e}"))?;
            let mut conversation = action.create_new_conversation(
                display_order,
                body_len,
                attachment_metadata.clone(),
            );
            conversation
                .save_using(tether)
                .await
                .inspect_err(|e| error!("Failed to create new conversation: {e}"))?;
            metadata.local_conversation_id = Some(conversation.local_id.unwrap());
            created_conversation = true;
            conversation.local_id.unwrap()
        };

        let message = if let Some(message_id) = metadata.local_message_id {
            debug!("Local message id is set, update");
            let Some(message) = Message::find_by_id(message_id, tether)
                .await
                .inspect_err(|e| error!("Failed to load message: {e}"))?
            else {
                return Err(AppError::MessageMissing(message_id).into());
            };

            // TODO(ET-1353): Update existing message
            let Some(_body_metadata) =
                MessageBodyMetadata::for_message(message_id, tether)
                    .await
                    .inspect_err(|e| error!("Failed to load message metadata: {e}"))?
            else {
                return Err(AppError::MessageMissing(message_id).into());
            };
            // TODO(ET-1353): Update existing message metadata

            message
        } else {
            debug!("Local message id is not set, creating new draft");
            let time = draft::create_timestamp();
            let display_order = Message::next_display_order(tether)
                .await
                .inspect_err(|e| error!("Failed to get next message display order: {e}"))?;
            let mut message = action.create_new_message(
                &address,
                attachment_metadata,
                body_len,
                time,
                display_order,
            );
            message.local_conversation_id = Some(conversation_id);
            message
                .save_using(tether)
                .await
                .inspect_err(|e| error!("Failed to save message: {e}"))?;

            let mut message_body_metadata = MessageBodyMetadata {
                local_message_id: Some(message.local_id.unwrap()),
                remote_message_id: None,
                header: "".to_string(),
                mime_type: action.mime_type,
                parsed_headers: Default::default(),
                attachments,
                row_id: None,
                stash: None,
            };

            message_body_metadata
                .save_using(tether)
                .await
                .inspect_err(|e| error!("Failed to save message body metadata: {e}"))?;

            Message::apply_label(
                local_draft_id,
                std::iter::once(message.local_id.unwrap()),
                tether,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to apply draft label to new message: {e}");
            })?;

            created_message = true;
            message
        };

        // Store body in cache.
        store_body_in_cache(ctx.messages_cache(), &message, &action.body, tether).inspect_err(
            |e| {
                error!("Failed to store draft body in cache :{e}");
            },
        )?;

        metadata.local_message_id = Some(message.local_id.unwrap());
        metadata.save_using(tether).await.inspect_err(|e| {
            error!("Failed to save draft metadata: {e}");
        })?;

        action.message_id = metadata.local_message_id;
        action.conversation_id = metadata.local_conversation_id;
        action.reply_mode = metadata.reply_mode;
        action.parent_id = metadata.local_parent_id;
        action.created_message = created_message;
        action.conversation_created = created_conversation;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: &MailUserContext,
        action: &mut Self::Action,
        tether: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // If create failed we need to wipe all new local resources so
        // they don't show up. Maybe keep them deleted until the remote
        // side finished.
        // If update failed we don't do anything.
        if action.conversation_created {
            // The conversation may not have been created.
            if let Some(id) = action.conversation_id {
                tether
                    .execute("DELETE FROM conversations WHERE local_id=?", params![id])
                    .await
                    .inspect_err(|e| error!("Failed to delete draft conversation: {e}"))?;
            }
        }

        if action.created_message {
            // The message may not have been created.
            if let Some(id) = action.message_id {
                tether
                    .execute("DELETE FROM messages WHERE local_id=?", params![id])
                    .await
                    .inspect_err(|e| error!("Failed to delete new draft message: {e}"))?;
            }
        }
        Ok(())
    }

    async fn apply_remote(
        &self,
        ctx: &MailUserContext,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        //TODO: detect if create or update and act accordingly
        // most of the code should be the same.
        let tether = stash.connection();

        let message_id = action.message_id.expect("Should be set");
        let conversation_id = action.conversation_id.expect("Should be set");

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

        let Some(mut message_body_metadata) = MessageBodyMetadata::for_message(message_id, &tether)
            .await
            .inspect_err(|e| {
                error!("Failed to load message body metadata for {message_id}: {e}")
            })?
        else {
            return Err(AppError::MessageBodyMetadataMissing(message_id).into());
        };

        // Load body.
        let key = CacheMessageKey::from_message(&message, &tether);
        let Some(mut message_body_reader) = ctx.messages_cache().get_item(&key)? else {
            return Err(AppError::MessageBodyMissing(message_id).into());
        };

        let remote_parent_id = if let Some(parent_id) = action.parent_id {
            let Some(remote_id) = parent_id
                .counterpart::<Message, _>(&tether)
                .await
                .inspect_err(|e| error!("Failed to resolve remote parent id: {e}"))?
            else {
                error!("Could not find parent message with id {parent_id}");
                return Err(AppError::MessageMissing(parent_id).into());
            };

            Some(remote_id)
        } else {
            None
        };

        let mut message_body = String::with_capacity(usize::try_from(message.size).unwrap_or(0));
        message_body_reader.read_to_string(&mut message_body)?;

        // Create draft on the server.
        let new_message = if message.remote_id.is_none() {
            Draft::remote_create(
                ctx,
                session,
                action.address_id.clone(),
                action.reply_mode.map_or(DraftAction::Reply, Into::into),
                &message,
                &message_body_metadata,
                &message_body,
                remote_parent_id,
            )
            .await
            .inspect_err(|e| {
                error!("Failed to create draft on remote: {e}");
            })?
        } else {
            todo!()
        };

        // Note: This section will be generalized as part of ET-1353 when
        // we implement draft updates.
        tether.transaction().await?;
        let row_id = message.row_id;

        // Update remote ids
        message.remote_id = Some(new_message.metadata.id.clone().into());
        message.remote_conversation_id = Some(new_message.metadata.conversation_id.clone().into());
        conversation.remote_id = Some(new_message.metadata.conversation_id.clone().into());

        // Because we can't have custom update function in stash we need to
        // first set the remote id on the message body metadata and then
        // we can save the metadata returned by the server.
        message_body_metadata.remote_message_id = message.remote_id.clone();
        message_body_metadata
            .save_using(&tether)
            .await
            .inspect_err(|e| error!("Failed to save message body metadata with remote id: {e}"))?;

        // Update conversation
        conversation
            .save_using(&tether)
            .await
            .inspect_err(|e| error!("Failed to update the conversation: {e}"))?;

        // Update message data
        let (mut message, mut new_message_body_metadata, _) =
            Message::from_api_data(new_message, &tether)
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
        new_message_body_metadata.local_message_id = Some(message_id);
        new_message_body_metadata.row_id = message_body_metadata.row_id;
        new_message_body_metadata
            .save_using(&tether)
            .await
            .inspect_err(|e| {
                error!("Failed to update message body metadata: {e}");
            })?;

        tether.commit().await?;

        Ok(())
    }
}

impl Save {
    fn create_new_message(
        &self,
        address: &Address,
        attachments: Vec<AttachmentMetadata>,
        body_len: u64,
        time: u64,
        display_order: u64,
    ) -> Message {
        let num_attachments = attachments.len();
        Message {
            local_id: None,
            remote_id: None,
            local_conversation_id: None,
            remote_conversation_id: None,
            local_address_id: address.local_id.unwrap(),
            remote_address_id: address.remote_id.clone().unwrap(),
            attachments_metadata: attachments,
            cc_list: to_message_addresses(&self.cc_list),
            bcc_list: to_message_addresses(&self.bcc_list),
            deleted: false,
            exclusive_location: None,
            expiration_time: 0,
            external_id: None,
            flags: Default::default(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![],
            num_attachments: num_attachments.try_into().unwrap_or_default(),
            display_order,
            reply_tos: Default::default(),
            sender: MessageAddress {
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
            to_list: to_message_addresses(&self.to_list),
            unread: false,
            custom_labels: vec![],
            cached: false,
            row_id: None,
            stash: None,
        }
    }

    fn create_new_conversation(
        &self,
        display_order: u64,
        body_len: u64,
        attachments: Vec<AttachmentMetadata>,
    ) -> Conversation {
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
            num_attachments: 0,
            num_messages: 0,
            num_unread: 0,
            display_order,
            recipients: Default::default(),
            senders: to_message_addresses(std::iter::once(&self.sender)),
            size: body_len,
            subject: self.subject.clone(),
            is_known: false,
            custom_labels: vec![],
            has_messages: false,
            row_id: None,
            stash: None,
        }
    }

    async fn attachments(&self, tether: &Tether) -> Result<Vec<Attachment>, StashError> {
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

fn to_message_addresses<'a>(addresses: impl IntoIterator<Item = &'a String>) -> MessageAddresses {
    MessageAddresses {
        value: addresses
            .into_iter()
            .map(|email| {
                //TODO(ET-1416): Resolve contact info.
                MessageAddress {
                    address: email.clone(),
                    bimi_selector: None,
                    display_sender_image: false,
                    is_proton: false,
                    is_simple_login: false,
                    name: String::new(),
                }
            })
            .collect(),
    }
}

/// Store the message body in the cache.
fn store_body_in_cache<A>(
    cache: &ProtonCache<CacheMessageConfig>,
    message: &Message,
    body: &str,
    interface: &A,
) -> Result<(), AppError>
where
    A: Into<AgnosticInterface> + Interface,
{
    let key = CacheMessageKey::from_message(message, interface);

    cache.add_item(key, body.as_bytes()).map_err(|e| {
        error!("Failed to store draft body in cache: {e}");
        AppError::Cache(e)
    })?;
    Ok(())
}

/// Resolve the Drafts local label id.
async fn local_draft_label_id<A>(interface: &A) -> Result<LocalId, MailContextError>
where
    A: Into<AgnosticInterface> + Interface,
{
    let Some(local_draft_label_id) = LabelId::drafts().counterpart::<Label, _>(interface).await?
    else {
        return Err(AppError::RemoteLabelDoesNotExist(LabelId::drafts()).into());
    };

    Ok(local_draft_label_id)
}
