mod actions;
mod events;
mod images;
mod initialization;
mod labels;

use crate::mail::MailSessionError;
use proton_mail_common as pmc;
use std::sync::Arc;

/// [`MailUserSession`] contains all the relevant information for an active user session, you
/// obtain one by completing the [`crate::mail::LoginFlow`] or restoring an existing session
/// with [`crate::mail::MailSession::user_context_from_session`].
///
/// # Initialization
/// [`MailUserSession`] *needs to be initialized ([`MailUserSession::initialize`]) once after a
/// new session is created*. This is required in order pre-load all the relevant user state.
/// No [`crate::mail::Mailbox`] instances should be created until then.
///
/// # Lifetime
/// This object needs to be kept alive for the duration of an active user session.
#[derive(uniffi::Object)]
pub struct MailUserSession {
    ctx: Arc<pmc::MailUserContext>,
}

impl MailUserSession {
    pub(crate) fn new(ctx: Arc<pmc::MailUserContext>) -> Arc<Self> {
        Arc::new(Self { ctx })
    }
    pub(crate) fn ctx(&self) -> Arc<pmc::MailUserContext> {
        Arc::clone(self.ctx)
    }
}

#[uniffi::export]
impl MailUserSession {
    /// Log out a session.
    pub async fn logout(&self) -> Result<(), MailSessionError> {
        self.ctx().logout().await?;
        Ok(())
    }

    /// Fork the current session.
    ///
    /// This call has to be made from a parent session, and forks the current
    /// logged-in user session in order to provide a new session for the same
    /// user.
    ///
    /// If successful, this will return the "Selector" string for the new
    /// session.
    ///
    /// # Errors
    ///
    /// Any of the [`MailSessionError::Http`] possibilities could be returned if
    /// there is a problem with the HTTP request.
    ///
    pub async fn fork(&self) -> Result<String, MailSessionError> {
        self.ctx
            .session()
            .fork()
            .await
            .map_err(MailSessionError::from)
    }
}
