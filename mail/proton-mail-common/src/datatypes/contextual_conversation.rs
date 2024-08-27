use crate::datatypes::{AttachmentMetadata, CustomLabel, ExclusiveLocation, MessageAddresses};
use crate::models::Conversation;
use proton_core_common::datatypes::{LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use stash::stash::{AgnosticInterface, Interface, StashError};

/// Contextual representation of a [`Conversation`] when it is opened for display
/// in a [`Label`].
///
/// The data contained in the [`ConversationLabel`] is superimposed over the
/// data in the [`Conversation`] to produce the correct information that needs
/// to be displayed to the client.
pub struct ContextualConversation {
    /// Local id of the conversation.
    pub local_id: LocalId,

    /// Remote id of the conversation.
    pub remote_id: Option<RemoteId>,

    /// Attachment metadata associated with this conversation.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// List of custom labels.
    pub custom_labels: Vec<CustomLabel>,

    /// Whether a snooze reminder should be displayed.
    pub display_snooze_reminder: bool,

    /// Order in the list this conversation should be displayed.
    pub display_order: u64,

    /// Exclusive location of the [`Conversation`] (e.g. Inbox, Archive, Outbox
    /// etc.). This field is auto-calculated, and not stored in the database.
    /// When the model is read from database, this field should be calculated,
    /// and always be [`Some`]. If it is [`None`], it means either that the
    /// model is not fully initialized or there is very nasty bug. Failed
    /// initialization is logged as an error, but flow is not impacted due to
    /// the fact that this is not a critical field.
    pub exclusive_location: Option<ExclusiveLocation>,

    /// Time at which this conversation expires.
    pub expiration_time: u64,

    /// Whether this conversation is starred.
    pub is_starred: bool,

    /// Number of attachments on the conversation.
    pub num_attachments: u64,

    /// Number of messages in this conversation.
    pub num_messages: u64,

    /// Number of unread messages in this conversation.
    pub num_unread: u64,

    /// Address of the recipients of the messages contained within.
    pub recipients: MessageAddresses,

    /// Address of all the senders in the messages.
    pub senders: MessageAddresses,

    /// Total size of all the messages.
    pub size: u64,

    /// Conversation subject.
    pub subject: String,

    /// Time of reception of the last message in this conversation.
    pub time: u64,

    /// TODO: Document this field
    pub snooze_time: u64,
}

impl ContextualConversation {
    /// Create a new instance for a `conversation` and the `local_label_id` where
    /// the contextual information should be applied.
    ///
    /// If the `local_label_id` is not present in the `conversation`, `None` is
    /// returned. This means that the conversation is not present in this label.
    pub fn new(conversation: Conversation, local_label_id: LocalId) -> Option<Self> {
        let label = conversation
            .labels
            .iter()
            .find(|&label| label.local_label_id == Some(local_label_id))?;

        let is_starred = conversation.is_starred();

        Some(Self {
            local_id: conversation.local_id.expect("Should be set"),
            remote_id: conversation.remote_id,
            attachments_metadata: conversation.attachments_metadata,
            custom_labels: conversation.custom_labels,
            display_order: conversation.display_order,
            display_snooze_reminder: conversation.display_snooze_reminder,
            exclusive_location: conversation.exclusive_location,
            expiration_time: label.context_expiration_time,
            is_starred,
            num_attachments: label.context_num_attachments,
            num_messages: label.context_num_messages,
            num_unread: label.context_num_unread,
            recipients: conversation.recipients,
            senders: conversation.senders,
            size: label.context_size,
            subject: conversation.subject,
            time: label.context_time,
            snooze_time: label.context_snooze_time,
        })
    }

    /// Load a conversation with `local_conversation_id` and the
    /// `local_label_id` where  the contextual information should be applied.
    ///
    /// If the `local_label_id` is not present in the `conversation`, `None` is
    /// returned. This means that the conversation is not present in this label.
    ///
    /// # Errors
    ///
    /// Returns error if conversation could not be loaded from the database.
    pub async fn load<A>(
        local_conversation_id: LocalId,
        local_label_id: LocalId,
        interface: &A,
    ) -> Result<Option<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(conversation) = Conversation::find_by_id(local_conversation_id, interface).await?
        else {
            return Ok(None);
        };

        Ok(Self::new(conversation, local_label_id))
    }
}
