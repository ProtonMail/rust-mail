mod composer;
mod conversations;
mod messages;
mod model;
mod paginator;
mod popups;

use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::{DecryptedMessage, MessagesState};
use crate::app_model::watcher::WatchHandle;
use crate::messages::Messages;
pub use model::Model;
use proton_core_common::datatypes::LocalId;
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::models::{Label, Message as MailMessage};
use proton_mail_common::Mailbox;

const ITEM_LIMIT: usize = 50;

pub enum Message {
    Sync(Mailbox),
    OpenConversationView(Mailbox, Label, ConversationsState),
    OpenMessageView(Mailbox, Label, MessagesState),
    OpenLabelSelectPopup,
    OpenMoveItemPopup(Item),
    OpenLabelItemPopup(Item),
    OpenUnlabelItemPopup(Item),
    SelectLabel(LocalId),
    ConversationState(ConversationMessage),
    LabelRefreshed(Label),
    #[allow(clippy::enum_variant_names)]
    MessageState(MessageMessage),
    OpenComposer(Composer),
    CloseComposer,
    NewLabelWatcher(WatchHandle),
    Composer(ComposerMessage),
}
/// Messages related to conversation actions.
pub enum ConversationMessage {
    MarkConversationRead(LocalId),
    MarkConversationUnread(LocalId),
    DeleteConversation(LocalId),
    MoveConversation(LocalId, LocalId),
    LabelConversation(LocalId, LocalId),
    UnlabelConversation(LocalId, LocalId),
    StarConversation(LocalId),
    UnstarConversation(LocalId),
    OpenConversation(LocalId),
    OpenConversationSuccess(Box<MessagesState>),
    OpenConversationFailed(anyhow::Error),
    Refreshed(Vec<ContextualConversation>),
    NextPage(Vec<ContextualConversation>),
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
    Refreshed(Vec<MailMessage>),
    NextPage(Vec<MailMessage>),
    DeleteMessage(LocalId),
    MoveMessage(LocalId, LocalId),
    LabelMessage(LocalId, LocalId),
    UnlabelMessage(LocalId, LocalId),
    MarkMessageRead(LocalId),
    MarkMessageUnread(LocalId),
    StarMessage(LocalId),
    UnstarMessage(LocalId),
}

impl From<MessageMessage> for Messages {
    fn from(value: MessageMessage) -> Self {
        Message::MessageState(value).into()
    }
}

/// Message related to the composer.
pub enum ComposerMessage {
    Save,
}

impl From<ComposerMessage> for Messages {
    fn from(value: ComposerMessage) -> Self {
        Message::Composer(value).into()
    }
}

pub enum Item {
    Conversation(LocalId),
    //TODO:message actions
    #[allow(dead_code)]
    Message(LocalId),
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Mailbox(value)
    }
}
