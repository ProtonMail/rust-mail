use crate::state::{DataLoadError, UserState};
use proton_api_mail::proton_api_core::http::HttpRequestError;
use proton_api_mail::proton_api_core::{LoginError, TotpSession};
use proton_mail_db::{LocalConversationWithContext, LocalLabel};

pub mod login;
pub mod mailbox;

pub enum AppEvents {
    Login(login::LoginEvents),
    Mailbox(mailbox::MailboxEvents),
}

impl AppEvents {
    pub fn login_failed(err: LoginError) -> Self {
        AppEvents::Login(login::LoginEvents::LoginFailed(err))
    }

    pub fn login_success(r: Result<UserState, DataLoadError>) -> Self {
        AppEvents::Login(login::LoginEvents::LoginSuccess(r))
    }

    pub fn login_needs_2fa(s: TotpSession) -> Self {
        AppEvents::Login(login::LoginEvents::LoginNeed2FA(s))
    }

    pub fn login_2fa_failed(err: HttpRequestError) -> Self {
        AppEvents::Login(login::LoginEvents::Login2FAFailed(err))
    }

    pub fn mailbox_label_load(r: Result<Vec<LocalLabel>, DataLoadError>) -> Self {
        AppEvents::Mailbox(mailbox::MailboxEvents::LoadLabels(r))
    }

    pub fn mailbox_conversation_load(
        r: Result<Vec<LocalConversationWithContext>, DataLoadError>,
    ) -> Self {
        AppEvents::Mailbox(mailbox::MailboxEvents::LoadConversations(r))
    }
}
