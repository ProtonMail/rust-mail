use crate::state::MailboxStateError;
use proton_mail_common::db::{LocalConversationId, LocalLabelId};
use proton_mail_common::MailUserContext;
use std::fmt::Formatter;

pub enum MailboxEvent {
    NewMailboxSession(MailUserContext),
    NewMailboxSessionInitialized,
    MailboxRefresh,
    LoadLabels(Result<(), MailboxStateError>),
    LoadConversations(Result<(), MailboxStateError>),
    LoadLabelRequest(LocalLabelId),
    PollEventLoop,
    ExecQueue,
    Logout,
    DeleteConversation(LocalConversationId),
    MarkConversationRead(LocalConversationId),
    MarkConversationUnread(LocalConversationId),
    LabelConversation(LocalConversationId, LocalLabelId),
    UnlabelConversation(LocalConversationId, LocalLabelId),
    MoveConversation(LocalConversationId, LocalLabelId),
}

// Custom debug formatter so that log doesn't implode with all the metadata.
impl std::fmt::Debug for MailboxEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MailboxEvent::NewMailboxSession(_) => {
                write!(f, "MailboxEvent::NewMailboxSession")
            }
            MailboxEvent::NewMailboxSessionInitialized => {
                write!(f, "MailboxEvent::NewMailboxSessionInitialized")
            }
            MailboxEvent::MailboxRefresh => {
                write!(f, "MailboxEvent::NewMailboxRefresh")
            }
            MailboxEvent::LoadLabels(_) => {
                write!(f, "MailboxEvent::LoadLabels")
            }
            MailboxEvent::LoadConversations(_) => {
                write!(f, "MailboxEvent::LoadConversations")
            }
            MailboxEvent::LoadLabelRequest(_) => {
                write!(f, "MailboxEvent::LoadLabelRequest")
            }
            MailboxEvent::PollEventLoop => {
                write!(f, "MailboxEvent::PollEventLoop")
            }
            MailboxEvent::ExecQueue => {
                write!(f, "MailboxEvent::ExecQueue")
            }
            MailboxEvent::Logout => {
                write!(f, "MailboxEvent::Logout")
            }
            MailboxEvent::DeleteConversation(id) => {
                write!(f, "MailboxEvent::DeleteConversation({id})")
            }
            MailboxEvent::MarkConversationRead(id) => {
                write!(f, "MailboxEvent::MarkConversationRead({id})")
            }
            MailboxEvent::MarkConversationUnread(id) => {
                write!(f, "MailboxEvent::MarkConversationUnread({id})")
            }
            MailboxEvent::LabelConversation(id, lid) => {
                write!(f, "MailboxEvent::LabelConversation({id}, {lid}")
            }
            MailboxEvent::UnlabelConversation(id, lid) => {
                write!(f, "MailboxEvent::UnlabelConversation({id}, {lid}")
            }
            MailboxEvent::MoveConversation(id, lid) => {
                write!(f, "MailboxEvent::MoveConversation({id}, {lid}")
            }
        }
    }
}
