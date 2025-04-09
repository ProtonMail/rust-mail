mod composer;
mod conversations;
mod messages;
mod model;
mod paginator;
mod popups;
mod search;

use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::{DecryptedMessage, MessagesState};
use crate::app_model::watcher::WatchHandle;
use crate::messages::Messages;
use messages::BlockOrUnblock;
pub use model::Model;
use proton_core_common::datatypes::{LocalIdMarker, LocalLabelId};
use proton_core_common::models::Label;
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::{
    ContextualConversation, LocalAttachmentId, LocalConversationId, LocalMessageId,
};
use proton_mail_common::draft::attachments::DraftAttachment;
use proton_mail_common::models::{Attachment, Message as MailMessage};
use search::{Search, SearchStatusBar};
use std::path::PathBuf;

const ITEM_LIMIT: usize = 50;

pub enum Message {
    Sync(Mailbox),
    OpenConversationView(Mailbox, Label, ConversationsState),
    OpenMessageView(Mailbox, Label, MessagesState),
    OpenSearchView(Mailbox, MessagesState),
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
    SearchSubmit(String),
    SearchPopup(Search),
    CloseSearchPopup,
    SearchStatusBar(SearchStatusBar),
    ClearSearchStatusBar,
    OpenContacts,
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
    MarkConversationRead(LocalConversationId),
    MarkConversationUnread(LocalConversationId),
    DeleteConversation(LocalConversationId),
    MoveConversation(LocalConversationId, LocalLabelId),
    LabelConversation(Box<LabelAs<LocalConversationId>>),
    StarConversation(LocalConversationId),
    UnstarConversation(LocalConversationId),
    OpenConversation(LocalConversationId),
    OpenConversationSuccess(Box<MessagesState>),
    OpenConversationFailed(anyhow::Error),
    Refreshed(Vec<ContextualConversation>),
    NextPage(Vec<ContextualConversation>),
    HasMore,
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
    BlockSender(String, BlockOrUnblock),
    HasMore,
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
    Discard,
    CreateAttachment(PathBuf),
    AddAttachment(Box<Attachment>),
    RemoveAttachment(LocalAttachmentId),
    RefreshAttachmentList,
    AttachmentListRefreshed(Vec<DraftAttachment>),
}

impl From<ComposerMessage> for Messages {
    fn from(value: ComposerMessage) -> Self {
        Message::Composer(value).into()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Item {
    Conversation(LocalConversationId),
    //TODO:message actions
    Message(LocalMessageId),
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Mailbox(value)
    }
}
