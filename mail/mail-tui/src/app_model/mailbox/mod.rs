mod conversations;
mod messages;
mod model;
mod popups;

pub use model::Model;
use proton_async::sync::mpsc::Sender;
use proton_core_common::db::DBResult;
use std::marker::PhantomData;

use crate::app::Command;
use crate::app_model::mailbox::messages::{DecryptedMessage, MessagesState};
use crate::messages::Messages;
use proton_core_common::db::proton_sqlite3::{InProcessTrackerService, Observable, Observed};
use proton_mail_common::db::{
    LabelItemCount, LocalConversation, LocalConversationId, LocalLabel, LocalLabelId,
    LocalMessageId, LocalMessageMetadata,
};
use proton_mail_common::exports::tracing::error;
use proton_mail_common::{Mailbox, MailboxObservableQueryBuilder};

const ITEM_LIMIT: usize = 50;

pub enum Message {
    Sync(Mailbox),
    OpenConversationView(Mailbox, LocalLabel),
    OpenMessageView(Mailbox, LocalLabel),
    OpenLabelSelectPopup,
    OpenMoveItemPopup(Item),
    OpenLabelItemPopup(Item),
    OpenUnlabelItemPopup(Item),
    SelectLabel(LocalLabelId),
    ConversationState(ConversationMessage),
    ItemCountRefreshed(LabelItemCount),
    #[allow(clippy::enum_variant_names)]
    MessageState(MessageMessage),
}
/// Messages related to conversation actions.
pub enum ConversationMessage {
    MarkConversationRead(LocalConversationId),
    MarkConversationUnread(LocalConversationId),
    DeleteConversation(LocalConversationId),
    MoveConversation(LocalConversationId, LocalLabelId),
    LabelConversation(LocalConversationId, LocalLabelId),
    UnlabelConversation(LocalConversationId, LocalLabelId),
    OpenConversation(LocalConversationId),
    OpenConversationResult(anyhow::Result<Box<MessagesState>>),
    Refreshed(Vec<LocalConversation>),
    CloseConversation,
}

impl From<ConversationMessage> for Messages {
    fn from(value: ConversationMessage) -> Self {
        Message::ConversationState(value).into()
    }
}

/// Messages related to message actions.
pub enum MessageMessage {
    OpenMessageBody,
    OpenMessageBodyResult(anyhow::Result<Box<DecryptedMessage>>),
    CloseMessageBody,
    Refreshed(Vec<LocalMessageMetadata>),
}

impl From<MessageMessage> for Messages {
    fn from(value: MessageMessage) -> Self {
        Message::MessageState(value).into()
    }
}

pub enum Item {
    Conversation(LocalConversationId),
    //TODO:message actions
    #[allow(dead_code)]
    Message(LocalMessageId),
}

pub trait ToObservableMessage<T>: Send + Sync + 'static {
    fn to_message(&self, value: DBResult<T>) -> Command<Messages>;
}

impl<T, F> ToObservableMessage<T> for F
where
    T: 'static,
    F: Fn(DBResult<T>) -> Command<Messages> + Send + Sync + 'static,
{
    fn to_message(&self, value: DBResult<T>) -> Command<Messages> {
        self(value)
    }
}
pub type BackgroundSender = Sender<Command<Messages>>;
pub struct LiveQueryBuilder<Q: Observable, T: ToObservableMessage<Q::Output>> {
    _p: PhantomData<Q>,
    converter: T,
    sender: BackgroundSender,
}

impl<Q: Observable, T: ToObservableMessage<Q::Output>> LiveQueryBuilder<Q, T> {
    pub fn new(converter: T, sender: Sender<Command<Messages>>) -> Self {
        Self {
            _p: PhantomData,
            converter,
            sender,
        }
    }
}

impl<Q: Observable, T: ToObservableMessage<Q::Output>> MailboxObservableQueryBuilder<Q>
    for LiveQueryBuilder<Q, T>
{
    type Output = Observed;

    fn build(self, tracker: InProcessTrackerService, query: Q) -> Self::Output {
        let converter = self.converter;
        let sender = self.sender;
        Observed::new(tracker, query, move |result: DBResult<Q::Output>| {
            if sender.send(converter.to_message(result)).is_err() {
                error!("Failed to send update from live query");
            }
        })
    }
}
impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Mailbox(value)
    }
}
