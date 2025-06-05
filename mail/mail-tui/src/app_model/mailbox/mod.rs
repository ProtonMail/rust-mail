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
use crate::app_model::watcher::TuiWatchHandle;
use crate::messages::Messages;
use chrono::{DateTime, Local};
use messages::BlockOrUnblock;
pub use model::MailboxModel;
use proton_core_common::datatypes::{LocalIdMarker, LocalLabelId};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::{
    ContextualConversation, LocalAttachmentId, LocalConversationId, LocalMessageId,
};
use proton_mail_common::draft::attachments::DraftAttachment;
use proton_mail_common::draft::compose::DraftAddressChangeOutput;
use proton_mail_common::models::{Attachment, LabelWithCounters, Message as MailMessage};
use proton_mail_common::proton_mail_api::proton_core_api::services::proton::AddressId;
use search::{Search, SearchStatusBar};
use std::path::PathBuf;

const ITEM_LIMIT: usize = 50;

pub enum Message {
    Sync(Mailbox),
    OpenConversationView(Mailbox, LabelWithCounters, ConversationsState),
    OpenMessageView(Mailbox, LabelWithCounters, MessagesState),
    OpenSearchView(Mailbox, MessagesState),
    OpenLabelSelectPopup,
    OpenMoveItemPopup(Item),
    OpenLabelItemPopup(Item),
    SelectLabel(LocalLabelId),
    ConversationState(ConversationMessage),
    LabelRefreshed(LabelWithCounters),
    #[allow(clippy::enum_variant_names)]
    MessageState(MessageMessage),
    OpenComposer(Composer),
    CloseComposer,
    NewLabelWatcher(TuiWatchHandle),
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
    DeleteAll(LocalLabelId),
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
    ReportPhishing(LocalMessageId),
    StarMessage(LocalMessageId),
    UnstarMessage(LocalMessageId),
    BlockSender(String, BlockOrUnblock),
    HasMore,
    CancelScheduleSend(LocalMessageId),
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
    ScheduleSend(DateTime<Local>),
    StartChangeAddress(AddressId),
    FinishChangeAddress(DraftAddressChangeOutput),
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
