use crate::datatypes::{
    AttachmentMetadata, CustomLabel, ExclusiveLocation, LabelType, MessageAddresses,
};
use crate::models::{Conversation, ConversationLabel, Label, Message};
use crate::AppError;
use indoc::formatdoc;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use stash::exports::ToSql;
use stash::orm::{Model, ResultsetChange};
use stash::params;
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

    /// Number of messages in this context.
    pub num_messages: u64,

    /// Number of unread messages in this context.
    pub num_unread: u64,

    /// Number of messages in this conversation.
    pub total_messages: u64,

    /// Number of unread messages in this conversation.
    pub total_unread: u64,

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
        let attachments_metadata = conversation.get_attachment_metadata();

        Some(Self {
            local_id: conversation.local_id.expect("Should be set"),
            remote_id: conversation.remote_id,
            attachments_metadata,
            custom_labels: conversation.custom_labels,
            display_order: conversation.display_order,
            display_snooze_reminder: conversation.display_snooze_reminder,
            exclusive_location: conversation.exclusive_location,
            expiration_time: label.context_expiration_time,
            is_starred,
            num_attachments: label.context_num_attachments,
            num_messages: label.context_num_messages,
            total_messages: conversation.num_messages,
            num_unread: label.context_num_unread,
            total_unread: conversation.num_unread,
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

    /// Retrieve all the conversations which are the label with `local_label_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn in_label<A>(
        local_label_id: LocalId,
        interface: &A,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(Conversation::in_label(local_label_id, interface, None)
            .await?
            .into_iter()
            .filter_map(|c| Self::new(c, local_label_id))
            .collect())
    }

    /// Retrieve the conversation with `local_conversation_id` in the
    /// context of `local_label_id` and its respective messages.
    ///
    /// This function also retrieve the messages from the server if
    /// they have never been synced before.
    ///
    /// # Error
    ///
    /// Returns error if the query failed, syncing the data failed or
    /// the conversation has no messages.
    #[tracing::instrument(level=tracing::Level::DEBUG,skip(interface,api))]
    pub async fn conversation_and_messages<A, PM>(
        local_conversation_id: LocalId,
        local_label_id: LocalId,
        interface: &A,
        api: &PM,
    ) -> Result<Option<ContextualConversationAndMessages>, AppError>
    where
        PM: ProtonMail,
        A: Into<AgnosticInterface> + Interface,
    {
        let label = Label::find_by_id(local_label_id, interface)
            .await?
            .ok_or(AppError::LabelNotFound(local_label_id))?;
        Conversation::sync_conversation_messages(local_conversation_id, interface, api).await?;
        let Some(conversation) =
            Self::load(local_conversation_id, local_label_id, interface).await?
        else {
            return Ok(None);
        };
        let messages = Message::in_conversation(local_conversation_id, interface, None).await?;
        let id_to_open =
            Conversation::message_id_to_open(local_conversation_id, &label, &messages)?;

        Ok(Some(ContextualConversationAndMessages {
            conversation,
            messages,
            message_id_to_open: id_to_open,
        }))
    }

    /// Watch a conversation with `local_conversation_id` in the context of
    /// `local_label_id`.
    ///
    /// A message is sent if the conversation or the conversation messages
    /// are updated.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub async fn watch_conversation_and_messages<A>(
        local_conversation_id: LocalId,
        interface: &A,
    ) -> Result<flume::Receiver<()>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        //TODO(ET-1088): Return ResultSetChange<ContextualConversation> instead of ()
        let (conv_sender, conv_receiver) = flume::unbounded();
        let (label_sender, label_receiver) = flume::unbounded();
        let (cb_sender, cb_receiver) = flume::unbounded();
        let (msg_sender, msg_receiver) = flume::unbounded();
        let (msg_label_sender, msg_label_receiver) = flume::unbounded();

        futures::try_join!(
            Conversation::find(
                "WHERE local_id = ?",
                params![local_conversation_id],
                interface,
                Some(conv_sender),
            ),
            ConversationLabel::find(
                "WHERE local_conversation_id =? ",
                params![local_conversation_id],
                interface,
                Some(label_sender),
            ),
            Message::in_conversation(local_conversation_id, interface, Some(msg_sender)),
            // Watching the message custom labels only here is enough as
            // the conversation's custom labels are a combination of all
            // the message labels. At this point we also have all
            // the conversation messages, so we don't need to handle the case
            // where they are not present.
            Label::find(
                formatdoc! {"
                    WHERE label_type=? AND local_id IN (
                        SELECT local_label_id FROM message_labels WHERE local_message_id IN (
                            SELECT local_id FROM messages WHERE local_conversation_id = ?
                        )
                    )
                "},
                params![LabelType::Label, local_conversation_id],
                interface,
                Some(msg_label_sender),
            )
        )?;

        tokio::spawn(async move {
            loop {
                tokio::select! {
                 label_result = label_receiver.recv_async() =>  {
                     if label_result.is_err() {
                         return;
                     }
                     if cb_sender.send_async(()).await.is_err() {
                         return;
                     }
                 }
                 conv_result = conv_receiver.recv_async() => {
                     if conv_result.is_err() {
                         return;
                     }
                     if cb_sender.send_async(()).await.is_err() {
                         return;
                     }
                 }
                 msg_result = msg_receiver.recv_async() => {
                     if msg_result.is_err() {
                         return;
                     }
                     if cb_sender.send_async(()).await.is_err() {
                         return;
                     }
                 }
                    msg_label_result = msg_label_receiver.recv_async()=> {
                     if msg_label_result.is_err() {
                         return;
                     }
                     if cb_sender.send_async(()).await.is_err() {
                         return;
                     }

                    }
                }
            }
        });

        Ok(cb_receiver)
    }

    /// Observe the conversations with `ids` for changes.
    ///
    /// Any time a change is detected a message is sent on the returned
    /// channel.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub async fn watch<A>(
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<flume::Receiver<()>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        //TODO(ET-1088): Return ResultSetChange<ContextualConversation> instead of ()
        let conversation_ids = ids.into_iter().collect::<Vec<_>>();
        let var_args = vec!["?"; conversation_ids.len()].join(",");
        let (conv_sender, conv_receiver) = flume::unbounded();
        let (conv_label_sender, con_label_receiver) = flume::unbounded();
        let (cb_sender, cb_receiver) = flume::unbounded();
        let (label_sender, label_receiver) = flume::unbounded();

        futures::try_join!(
            ConversationLabel::find(
                format!("WHERE local_conversation_id IN ({})", var_args),
                conversation_ids
                    .iter()
                    .map(|id| -> Box<dyn ToSql + Send> { Box::new(*id) })
                    .collect(),
                interface,
                Some(conv_label_sender),
            ),
            Conversation::find(
                format!("WHERE local_id IN ({})", var_args),
                conversation_ids
                    .iter()
                    .map(|id| -> Box<dyn ToSql + Send> { Box::new(*id) })
                    .collect(),
                interface,
                Some(conv_sender),
            ),
            Label::find(
                formatdoc! {"
                    WHERE label_type=? AND label_id IN (
                        SELECT local_label_id FROM conversation_labels
                        WHERE local_conversation_id IN ({args})
                        AND deleted = 0
                    )
                ", args=var_args},
                std::iter::once(Box::new(LabelType::Label) as Box<dyn ToSql + Send>)
                    .chain(
                        conversation_ids
                            .iter()
                            .map(|id| -> Box<dyn ToSql + Send> { Box::new(*id) })
                    )
                    .collect(),
                interface,
                Some(label_sender),
            )
        )?;

        tokio::spawn(async move {
            Self::watch_task(cb_sender, con_label_receiver, conv_receiver, label_receiver).await
        });

        Ok(cb_receiver)
    }

    /// Observe the conversations which have the given `label_id`.
    ///
    /// Any time a change is detected a message is sent on the returned
    /// channel.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub async fn watch_in_label<A>(
        label_id: LocalId,
        interface: &A,
    ) -> Result<(Vec<ContextualConversation>, flume::Receiver<()>), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        //TODO(ET-1088): Return ResultSetChange<ContextualConversation> instead of ()
        let (conv_sender, conv_receiver) = flume::unbounded();
        let (conv_label_sender, conv_label_receiver) = flume::unbounded();
        let (label_sender, label_receiver) = flume::unbounded();
        let (cb_sender, cb_receiver) = flume::unbounded();

        let (_, conversations, _) = futures::try_join!(
            ConversationLabel::find(
                "WHERE local_label_id =? AND deleted = 0",
                params![label_id],
                interface,
                Some(conv_label_sender),
            ),
            Conversation::in_label(label_id, interface, Some(conv_sender),),
            Label::find(
                formatdoc! {"
                    WHERE label_type=? AND local_id IN (
                        SELECT local_label_id FROM conversation_labels
                        WHERE local_conversation_id IN (
                            SELECT local_conversation_id FROM conversation_labels
                                WHERE local_label_id=? AND deleted = 0
                        )
                    )
                "},
                params![LabelType::Label, label_id],
                interface,
                Some(label_sender),
            )
        )?;

        tokio::spawn(async move {
            Self::watch_task(
                cb_sender,
                conv_label_receiver,
                conv_receiver,
                label_receiver,
            )
            .await
        });

        let conversations = conversations
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, label_id))
            .collect();

        Ok((conversations, cb_receiver))
    }

    // Shared implementation to observe the labels.
    async fn watch_task(
        sender: flume::Sender<()>,
        conv_label_receiver: flume::Receiver<ResultsetChange<ConversationLabel, LocalId>>,
        conv_receiver: flume::Receiver<ResultsetChange<Conversation, LocalId>>,
        custom_label_receiver: flume::Receiver<ResultsetChange<Label, LocalId>>,
    ) {
        loop {
            tokio::select! {
                conv_label_result = conv_label_receiver.recv_async() =>  {
                    if conv_label_result.is_err() {
                        return;
                    }
                    if sender.send_async(()).await.is_err() {
                        return;
                    }
                }
                custom_label_result = custom_label_receiver.recv_async() =>  {
                    if custom_label_result.is_err() {
                        return;
                    }
                    if sender.send_async(()).await.is_err() {
                        return;
                    }
                }
                conv_result = conv_receiver.recv_async() => {
                    if conv_result.is_err() {
                        return;
                    }
                    if sender.send_async(()).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// Result of calling [`ContextualConversation::conversation_and_messages`];
pub struct ContextualConversationAndMessages {
    /// The conversation
    pub conversation: ContextualConversation,
    /// The conversation's messages.
    pub messages: Vec<Message>,
    /// The id of message to display first.
    pub message_id_to_open: LocalId,
}
