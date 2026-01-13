mod composer;
mod conversations;
mod messages;
mod model;
mod popups;
mod scroller;
mod search;

use crate::app::Command;
use crate::app_model::mailbox::composer::Composer;
use crate::app_model::mailbox::conversations::ConversationsState;
use crate::app_model::mailbox::messages::{DecryptedMessage, MessagesState};
use crate::app_model::watcher::TuiWatchHandle;
use crate::messages::Messages;
use anyhow::Context;
use chrono::{DateTime, Local};
use composer::recipient_list::TuiRecipientList;
use messages::BlockOrUnblock;
pub use model::MailboxModel;
use proton_core_common::datatypes::{LocalIdMarker, LocalLabelId, Refresh, UnixTimestamp};
use proton_crypto_inbox::lock_icon::UiLock;
use proton_mail_api::proton_core_api::services::proton::{AddressId, PrivateEmail};
use proton_mail_common::datatypes::{
    ContextualConversation, LocalAttachmentId, LocalConversationId, LocalMessageId,
};
use proton_mail_common::decrypted_message::PrivacyLockBuilder;
use proton_mail_common::draft::RecipientGroupId;
use proton_mail_common::draft::attachments::DraftAttachment;
use proton_mail_common::draft::recipients::{Recipient, RecipientList};
use proton_mail_common::models::{Attachment, LabelWithCounters, Message as MailMessage};
use proton_mail_common::{MailUserContext, Mailbox, RsvpEvent};
use search::{Search, SearchStatusBar};
use secrecy::SecretString;
use std::path::PathBuf;
use std::sync::Arc;

const ITEM_LIMIT: usize = 50;

pub enum Message {
    Sync(Mailbox),
    OpenConversationView(Mailbox, LabelWithCounters, ConversationsState),
    OpenMessageView(Mailbox, LabelWithCounters, MessagesState),
    OpenSearchView(Mailbox, MessagesState),
    OpenLabelSelectPopup,
    OpenMoveItemsPopup(Items),
    OpenLabelItemPopup(Items),
    OpenSnoozePopup(Items),
    OpenCustomSnoozePopup(Vec<LocalConversationId>, LocalLabelId),
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
    ForcePollEventStart,
    ForcePollEventFinish,
    OpenUserFeatureFlags,
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
    MarkRead(Vec<LocalConversationId>),
    MarkUnread(Vec<LocalConversationId>),
    Snooze(Vec<LocalConversationId>, UnixTimestamp, LocalLabelId),
    Unsnooze(Vec<LocalConversationId>, LocalLabelId),
    DeletePermanently(Vec<LocalConversationId>),
    MoveTo(Vec<LocalConversationId>, LocalLabelId),
    LabelAs(Box<LabelAs<LocalConversationId>>),
    Star(Vec<LocalConversationId>),
    Unstar(Vec<LocalConversationId>),
    Open(LocalConversationId),
    OpenSuccess(Box<MessagesState>),
    OpenFailed(anyhow::Error),
    Close,
    NextPage(Vec<ContextualConversation>),
    ReplaceFrom(usize, Vec<ContextualConversation>),
    ReplaceBefore(usize, Vec<ContextualConversation>),
    ReplaceRange(usize, usize, Vec<ContextualConversation>),
    HasMore,
    DeleteAll(LocalLabelId),
    ScrollerFetchNewStart,
    ScrollerFetchNewEnd,
}

impl From<ConversationMessage> for Messages {
    fn from(value: ConversationMessage) -> Self {
        Message::ConversationState(value).into()
    }
}

/// Messages related to message actions.
pub enum MessageMessage {
    OpenBody { show_loading: bool },
    OpenBodyResult(anyhow::Result<(Box<DecryptedMessage>, PrivacyLockBuilder)>),
    CloseBody,
    ReplaceFrom(usize, Vec<MailMessage>),
    ReplaceBefore(usize, Vec<MailMessage>),
    ReplaceRange(usize, usize, Vec<MailMessage>),
    Refreshed(Vec<MailMessage>),
    NextPage(Vec<MailMessage>),
    DeletePermanently(Vec<LocalMessageId>),
    MoveTo(Vec<LocalMessageId>, LocalLabelId),
    LabelAs(Box<LabelAs<LocalMessageId>>),
    MarkRead(Vec<LocalMessageId>),
    MarkUnread(Vec<LocalMessageId>),
    ReportPhishing(LocalMessageId),
    Star(Vec<LocalMessageId>),
    Unstar(Vec<LocalMessageId>),
    BlockSender(PrivateEmail, BlockOrUnblock),
    HasMore,
    CancelScheduleSend(LocalMessageId),
    UpdateRsvp(Box<RsvpEvent>),
    ScrollerFetchNewStart,
    ScrollerFetchNewEnd,
    UpdatePrivacyLock(Option<UiLock>),
}

impl<I: Into<Messages>> From<I> for Command<Messages> {
    fn from(value: I) -> Self {
        let v = value.into();
        Command::Message(v)
    }
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
    RetryAttachmentOp(LocalAttachmentId),
    SwapDisposition(LocalAttachmentId),
    RefreshAttachmentList,
    AttachmentListRefreshed(Vec<DraftAttachment>),
    ScheduleSend(DateTime<Local>),
    StartChangeAddress((String, AddressId)),
    FinishChangeAddress { sender: String, body: String },
    SetPasswordProtection(SecretString, Option<String>),
    SetExpirationTime(DateTime<Local>),
    OpenRecipientList(RecipientGroupId),
    ShowRecipientList(TuiRecipientList),
    RecipientList(RecipientListMessage),
    CloseRecipientList,
}

impl From<ComposerMessage> for Messages {
    fn from(value: ComposerMessage) -> Self {
        Message::Composer(value).into()
    }
}

pub enum RecipientListMessage {
    ChangeSelection,
    AddRecipient(String),
    DeleteRecipient(Recipient),
    UpdateRecipients(RecipientGroupId, RecipientList),
}

impl From<RecipientListMessage> for Messages {
    fn from(value: RecipientListMessage) -> Self {
        Message::Composer(ComposerMessage::RecipientList(value)).into()
    }
}

#[derive(Debug, Clone)]
pub enum Items {
    Conversation(Vec<LocalConversationId>),
    //TODO:message actions
    Message(Vec<LocalMessageId>),
}

impl From<Message> for Messages {
    fn from(value: Message) -> Self {
        Self::Mailbox(value)
    }
}

pub fn refresh(ctx: Arc<MailUserContext>) -> Command<Messages> {
    Command::batch([
        Command::message(Messages::DisplayInfo(
            Some("Event Loop referesh".to_owned()),
            "Starting full refresh...".to_owned(),
        )),
        Command::from_future(async move {
            ctx.refresh_action(Refresh::All)
                .await
                .context("Event loop refresh")?;
            Ok(())
        }),
    ])
}

pub fn poll_event_loop(ctx: Arc<MailUserContext>) -> Command<Messages> {
    Command::batch([
        Message::ForcePollEventStart.into(),
        Command::task(async move {
            let _ = ctx.force_event_loop_poll_and_wait().await;
            Message::ForcePollEventFinish.into()
        }),
    ])
}
