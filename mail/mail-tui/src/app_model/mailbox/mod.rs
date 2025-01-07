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
use proton_core_common::datatypes::{LocalId, LocalIdMarker, LocalLabelId};
use proton_mail_common::datatypes::{ContextualConversation, LocalMessageId};
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
pub struct LabelAs<T: LocalIdMarker> {
    pub source_label_id: LocalLabelId,
    pub item_ids: Vec<T>,
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
    LabelConversation(Box<LabelAs<LocalId>>),
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
    DeleteMessage(LocalMessageId),
    MoveMessage(LocalMessageId, LocalLabelId),
    LabelMessage(Box<LabelAs<LocalMessageId>>), // TODO: Handle selection
    MarkMessageRead(LocalMessageId),
    MarkMessageUnread(LocalMessageId),
    StarMessage(LocalMessageId),
    UnstarMessage(LocalMessageId),
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
    Message(LocalMessageId),
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Mailbox(value)
    }
}
