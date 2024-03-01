use crate::app::AppLocalDispatcher;
use crate::events::mailbox::MailboxEvent;
use crate::events::session::SessionEvent;
use crate::events::AppEvent;
use crate::state::AppState;
use crate::views::LoginView;
use anyhow::anyhow;
use proton_mail_common::proton_api_mail::proton_api_core::exports::tracing::debug;
use proton_mail_common::proton_core_common::proton_core_db::EncryptedUserSession;
use proton_mail_common::{MailContext, MailContextError, MailContextResult, MailUserContext};

pub struct SessionState {
    sessions: Vec<EncryptedUserSession>,
    session_selected: bool,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            session_selected: true,
        }
    }

    pub fn sessions(&self) -> &[EncryptedUserSession] {
        &self.sessions
    }

    pub fn handle_event(
        &mut self,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        event: SessionEvent,
        mailbox_context: &MailContext,
    ) {
        match event {
            SessionEvent::LoadSessions => {
                if let Err(e) = self.load_sessions(mailbox_context) {
                    dispatcher.set_error("Session Error", e);
                }
                if self.sessions.is_empty() {
                    dispatcher.push_view(LoginView::new());
                } else {
                    self.session_selected = false;
                }
            }
            SessionEvent::SelectSession(index) => {
                if self.session_selected {
                    return;
                }
                self.session_selected = true;
                match self.new_session_from_index(mailbox_context, index) {
                    Ok(context) => {
                        dispatcher.queue_event(MailboxEvent::NewMailboxSession(context));
                    }
                    Err(e) => {
                        dispatcher.set_error("Session Error", e);
                    }
                }
                self.session_selected = false;
            }
            SessionEvent::NewSession => {
                dispatcher.push_view(LoginView::new());
            }
        }
    }

    fn load_sessions(&mut self, mail_context: &MailContext) -> MailContextResult<()> {
        self.sessions = mail_context.get_sessions()?;
        debug!("Found {} sessions", self.sessions.len());
        Ok(())
    }

    fn new_session_from_index(
        &self,
        mail_context: &MailContext,
        index: usize,
    ) -> MailContextResult<MailUserContext> {
        if index >= self.sessions.len() {
            return Err(MailContextError::Other(anyhow!("Invalid session index")));
        }

        mail_context.user_context_from_session(&self.sessions[index], None)
    }
}
