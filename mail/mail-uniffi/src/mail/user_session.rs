mod actions;
mod events;
mod images;
mod initialization;
mod labels;

use crate::errors::{UserSessionError, VoidSessionResult};
use crate::{core::datatypes::User, uniffi_async};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::MailUserContext;
use stash::stash::Stash;
use std::sync::Arc;

/// [`MailUserSession`] represents an active user session.
///
/// This type contains all the relevant information for an active user session.
/// You obtain one by completing the [`crate::mail::LoginFlow`] or restoring an existing session
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
    /// Get the User ID of the current user.
    #[must_use]
    pub fn user_id(&self) -> String {
        self.ctx.user_context().user_id().to_owned().into_inner()
    }

    /// Get the Session ID of the current user's session.
    #[must_use]
    pub fn session_id(&self) -> String {
        self.ctx.user_context().session_id().to_owned().into_inner()
    }

    /// Log out a session.
    pub async fn logout(&self) -> VoidSessionResult {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            ctx.logout().await?;

            Result::<_, RealProtonMailError>::Ok(())
        })
        .await
        .map_err(UserSessionError::from)
        .into()
    }
}

#[proton_uniffi_macros::export_result]
impl MailUserSession {
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
    pub async fn fork(&self) -> Result<String, UserSessionError> {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            ctx.session()
                .fork_with_version("web-account-lite".to_owned())
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Provides a way to get the datatypes::User FFI instance.
    ///
    /// # Errors
    ///
    /// Either when MailSessionError::Stash occurs or somehow the user is missing.
    pub async fn user(&self) -> Result<User, UserSessionError> {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            let user = ctx.user().await?;
            Result::<_, RealProtonMailError>::Ok(user.into())
        })
        .await
        .map_err(UserSessionError::from)
    }
}

impl MailUserSession {
    /// Get the connection to the user database
    #[must_use]
    pub fn user_stash(&self) -> &Stash {
        self.ctx.user_stash()
    }
}
