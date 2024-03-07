use crate::state::MailboxStateError;
use proton_mail_common::proton_mail_db::LocalLabelId;
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
    Logout,
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
            MailboxEvent::Logout => {
                write!(f, "MailboxEvent::Logout")
            }
        }
    }
}
