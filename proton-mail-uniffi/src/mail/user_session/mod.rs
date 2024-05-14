mod actions;
mod events;
mod images;
mod initialization;
mod labels;
use crate::mail::{map_task_join_error, MailSessionError, MailSessionResult};
use proton_mail_common as pmc;
use std::future::Future;
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

    /// Helper function to hide implementation details of how to run async code with
    /// uniffi.
    pub(crate) async fn uniffi_async<T, F>(&self, f: F) -> Result<T, MailSessionError>
    where
        T: Send + 'static,
        F: Future<Output = MailSessionResult<T>> + Send + 'static,
    {
        self.ctx
            .mail_context()
            .async_runtime()
            .spawn(f)
            .await
            .map_err(map_task_join_error)?
    }
}

#[uniffi::export]
impl MailUserSession {
    /// Log out a session.
    pub async fn logout(&self) -> Result<(), MailSessionError> {
        let ctx = self.ctx().clone();
        self.uniffi_async(async move { Ok(ctx.logout().await?) })
            .await
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
        // The handling of this async call is super-ugly, but aligns with the
        // code elsewhere for now
        let ctx = self.ctx.clone();
        let handle = self
            .ctx
            .mail_context()
            .async_runtime()
            .spawn(async move { ctx.session().fork().await });
        match handle.await {
            Ok(result) => result.map_err(MailSessionError::from),
            Err(e) => Err(map_task_join_error(e)),
        }
    }
}
