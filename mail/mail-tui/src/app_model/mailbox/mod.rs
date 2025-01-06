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
use proton_core_common::datatypes::{LocalId, LocalLabelId};
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
    SelectLabel(LocalLabelId),
    ConversationState(ConversationMessage),
    LabelRefreshed(Label),
    #[allow(clippy::enum_variant_names)]
    MessageState(MessageMessage),
    OpenComposer(Composer),
    CloseComposer,
    NewLabelWatcher(WatchHandle),
    Composer(ComposerMessage),
}
pub struct LabelAs {
    pub source_label_id: LocalLabelId,
    pub item_ids: Vec<LocalId>,
    pub selected_label_ids: Vec<LocalLabelId>,
    pub partially_selected_label_ids: Vec<LocalLabelId>,
    pub must_archive: bool,
}

/// Messages related to conversation actions.
pub enum ConversationMessage {
    MarkConversationRead(LocalId),
    MarkConversationUnread(LocalId),
    DeleteConversation(LocalId),
    MoveConversation(LocalId, LocalLabelId),
    LabelConversation(Box<LabelAs>),
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
    MoveMessage(LocalId, LocalLabelId),
    LabelMessage(Box<LabelAs>), // TODO: Handle selection
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
    Send,
}

impl From<ComposerMessage> for Messages {
    fn from(value: ComposerMessage) -> Self {
        Message::Composer(value).into()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Item {
    Conversation(LocalId),
    //TODO:message actions
    Message(LocalId),
}

impl Item {
    pub fn get_id(self) -> LocalId {
        match self {
            Item::Message(local_id) | Item::Conversation(local_id) => local_id,
        }
    }
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Mailbox(value)
    }
}
