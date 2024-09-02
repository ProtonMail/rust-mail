mod actions;
mod events;
mod images;
mod initialization;
mod labels;

use crate::{
    core::{datatypes::User, StoredSessionState},
    mail::{MailSessionError, MailSessionResult},
    uniffi_async,
};
use futures::TryFutureExt;
use proton_mail_common::{MailContextError, MailUserContext};
use stash::stash::Stash;
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
    ctx: Arc<MailUserContext>,
}

impl MailUserSession {
    pub(crate) fn new(ctx: Arc<MailUserContext>) -> Arc<Self> {
        Arc::new(Self { ctx })
    }
    pub(crate) fn ctx(&self) -> Arc<MailUserContext> {
        Arc::clone(&self.ctx)
    }
}

#[uniffi::export]
impl MailUserSession {
    /// Log out a session.
    pub async fn logout(&self) -> MailSessionResult<()> {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            ctx.logout().await?;
            Ok(())
        })
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
    pub async fn fork(&self) -> MailSessionResult<String> {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            ctx.session()
                .fork_with_version("web-account-lite".to_owned())
                .await
                .map_err(MailSessionError::from)
        })
        .await
    }

    /// Provides a way to get the datatypes::User FFI instance.
    ///
    /// # Errors
    ///
    /// Either when MailSessionError::Stash occurs or somehow the user is missing.
    pub async fn user(&self) -> MailSessionResult<User> {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            let user = ctx.user().await?;
            Ok(user.into())
        })
        .await
    }

    /// Get the state of the session.
    ///
    /// If the session has no state (i.e. it was never marked as active), this will return `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn state(&self) -> MailSessionResult<Option<Arc<StoredSessionState>>> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            let Some(state) = ctx
                .user_context()
                .state()
                .map_err(MailContextError::from)
                .await?
            else {
                return Ok(None);
            };

            Ok(Some(StoredSessionState::new(state)))
        })
        .await
    }

    /// Mark this session as active.
    ///
    /// This updates the last active timestamp of this session in the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn set_active(&self) -> MailSessionResult<()> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            Ok(ctx
                .user_context()
                .set_active()
                .map_err(MailContextError::from)
                .await?)
        })
        .await
    }

    /// Return whether the session is active.
    ///
    /// A session is considered active if it is the most recent session for the user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn is_active(&self) -> MailSessionResult<bool> {
        let ctx = self.ctx.clone();

        uniffi_async(async move {
            Ok(ctx
                .user_context()
                .is_active()
                .map_err(MailContextError::from)
                .await?)
        })
        .await
    }
}

impl MailUserSession {
    /// Get the connection to the user database
    #[must_use]
    pub fn user_stash(&self) -> &Stash {
        self.ctx.user_stash()
    }
}
