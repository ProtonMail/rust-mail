use login::LoginEvent;
use mailbox::MailboxEvent;
use session::SessionEvent;

pub mod login;
pub mod mailbox;
pub mod session;

#[derive(Debug)]
pub enum AppEvent {
    Login(LoginEvent),
    Mailbox(MailboxEvent),
    Session(SessionEvent),
}

impl From<LoginEvent> for AppEvent {
    fn from(value: LoginEvent) -> Self {
        Self::Login(value)
    }
}

impl From<MailboxEvent> for AppEvent {
    fn from(value: MailboxEvent) -> Self {
        AppEvent::Mailbox(value)
    }
}

impl From<SessionEvent> for AppEvent {
    fn from(value: SessionEvent) -> Self {
        AppEvent::Session(value)
    }
}
