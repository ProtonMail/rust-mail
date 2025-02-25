mod events;
mod images;
mod initialization;
mod labels;

use crate::core::datatypes::ConnectionStatus;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ActionError, ProtonError, UserSessionError, VoidSessionResult};
use crate::mail::state::MailUserContextPtr;
use crate::{async_runtime, spawn_async, LiveQueryCallback, MapIntoResult};
use crate::{
    core::datatypes::{AccountDetails, Id, User},
    uniffi_async,
};
use futures::TryFutureExt;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::MailUserContext;
use stash::stash::Stash;
use std::sync::Arc;

use super::datatypes::AttachmentMetadata;

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
    ctx: MailUserContextPtr,
}

impl MailUserSession {
    pub(crate) fn new(ctx: MailUserContextPtr) -> Arc<Self> {
        Arc::new(Self { ctx })
    }

    /// Get a clone of the inner weak reference to the user context.
    pub(crate) fn ptr(&self) -> MailUserContextPtr {
        self.ctx.clone()
    }

    /// Get a strong reference to the inner user context.
    pub(crate) fn ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.upgrade().ok_or(UnexpectedError::Internal)?)
    }

    /// Take ownership of the inner user context.
    pub(crate) fn take_ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.consume().ok_or(UnexpectedError::Internal)?)
    }

    /// Get the connection to the user database
    pub(crate) fn user_stash(&self) -> Result<Stash, ProtonError> {
        Ok(self.ctx()?.user_stash().to_owned())
    }
}

#[uniffi_export]
impl MailUserSession {
    /// Get the User ID of the current user.
    #[must_use]
    pub fn user_id(&self) -> Result<String, ProtonError> {
        Ok(self.ctx()?.user_id().to_owned().into_inner())
    }

    /// Get the Session ID of the current user's session.
    #[must_use]
    pub fn session_id(&self) -> Result<String, ProtonError> {
        Ok(self.ctx()?.session_id().to_owned().into_inner())
    }

    /// Log out a session.
    #[returns(VoidSessionResult)]
    pub async fn logout(&self) -> Result<(), UserSessionError> {
        let ctx = self.take_ctx()?;

        uniffi_async(async move { ctx.logout().map_err(RealProtonMailError::from).await })
            .await
            .map_err(UserSessionError::from)
            .into()
    }
}

#[uniffi_export]
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
        let ctx = self.ctx()?;
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
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let user = ctx.user().await?;
            Result::<_, RealProtonMailError>::Ok(user.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Retrieves the account details for the current user session.
    ///
    /// Returns the user's account details (name, email and avatar information) or an error if the operation fails.
    ///
    /// # Errors
    /// - Returns `UserSessionError` if the account details cannot be retrieved.
    pub async fn account_details(&self) -> Result<AccountDetails, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let account_details = ctx.account_details().await?;
            Result::<_, RealProtonMailError>::Ok(account_details.into())
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Loads the metadata and file path for the given local [`attachment_id`]
    /// into a [`DecryptedAttachment`].
    ///
    /// If the attachment is not present on the device it is retrieved from
    /// the server, decrypted and stored in the cache.
    ///
    /// Additionally, attempts to verify any attached signatures with the
    /// sender's keys. The result can be accessed via the [`VerificationResult`]
    /// result return type.
    ///
    /// # Warning
    ///
    /// Signature verification is currently always failing since no sender keys
    /// are fetched yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the encrypted attachment fetching or decryption fails.
    /// Signature verification failures are not returned as errors.
    pub async fn get_attachment(
        &self,
        local_attachment_id: Id,
    ) -> Result<DecryptedAttachment, ActionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            ctx.get_attachment(local_attachment_id.into())
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_into()
    }

    /// Get the connection status of the current user session.
    ///
    /// The method will return the current connection status of the user session.
    /// Underlying it will ping the Proton server with one second timeout to check
    /// if the connection can be established.
    ///
    /// The connection status can be one of the following:
    /// - `ConnectionStatus::Online`: The application is online.
    /// - `ConnectionStatus::Offline`: The application is offline.
    /// - `ConnectionStatus::ServerUnreachable`: The application is online but the server is unreachable.
    ///
    pub async fn connection_status(&self) -> Result<ConnectionStatus, UserSessionError> {
        let ctx = self.ctx()?;
        uniffi_async(async move {
            let status = ctx.connection_status().await.into();
            // unfortunatelly there is join error here which need to be handled
            Result::<ConnectionStatus, RealProtonMailError>::Ok(status)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Execute callback when connection status is online
    ///
    /// The method will execute callback immediately when current status is online
    /// otherwise it will wait till the status is online again and then execute callback
    ///
    pub fn execute_when_online(&self, callback: Box<dyn LiveQueryCallback>) {
        let Ok(ctx) = self.ctx() else {
            tracing::error!("Cannot obtain context, callback will not be executed");
            return;
        };

        spawn_async(ctx.clone(), async move {
            ctx.session().wait_for_online().await;
            let callback = move || callback.on_update();
            _ = async_runtime().spawn_blocking(callback).await;
        });
    }
}

impl From<proton_mail_common::DecryptedAttachment> for DecryptedAttachment {
    fn from(value: proton_mail_common::DecryptedAttachment) -> Self {
        Self {
            attachment_metadata: value.attachment_metadata.into(),
            data_path: value.data_path.to_str().expect("valid path").to_owned(),
        }
    }
}

/// Returned by [`Mailbox::get_attachment`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct DecryptedAttachment {
    /// Metadata of the decrypted attachment.
    pub attachment_metadata: AttachmentMetadata,
    /// The attachment content.
    pub data_path: String,
}
