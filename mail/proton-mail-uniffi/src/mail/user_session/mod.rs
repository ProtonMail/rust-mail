mod actions;
mod events;
mod images;
mod initialization;
mod labels;
mod settings;

use crate::mail::{map_task_join_error, MailSessionError};
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
    ctx: pmc::MailUserContext,
}

impl MailUserSession {
    pub(crate) fn new(ctx: pmc::MailUserContext) -> Arc<Self> {
        Arc::new(Self { ctx })
    }
    pub(crate) fn ctx(&self) -> &pmc::MailUserContext {
        &self.ctx
    }
}

#[uniffi::export]
impl MailUserSession {
    /// Log out a session.
    pub async fn logout(&self) -> Result<(), MailSessionError> {
        let ctx = self.ctx().clone();
        let handle = self
            .ctx
            .mail_context()
            .async_runtime()
            .spawn(async move { ctx.logout().await });
        handle.await.map_err(map_task_join_error)??;
        Ok(())
    }
}
