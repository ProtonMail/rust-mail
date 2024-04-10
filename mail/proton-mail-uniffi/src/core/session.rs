use proton_mail_common::exports::thiserror;
use proton_mail_common::proton_api_mail::proton_api_core::domain::{Uid, UserId};
use proton_mail_common::proton_api_mail::proton_api_core::http::HttpRequestError;
use proton_mail_common::proton_core_common::db::EncryptedUserSession;
use proton_mail_common::proton_core_common::{CoreSessionCallback, CoreSessionError};
use std::sync::Arc;

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum SessionError {
    #[error("Database Error: {0}")]
    DB(String),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(String),
    #[error("Keychain has no encryption key")]
    KeyChainHasNoKey,
    #[error("Other: {0}")]
    Other(String),
    #[error("Http: {0}")]
    Http(String),
}

#[uniffi::export(callback_interface)]
pub trait SessionCallback: Send + Sync {
    /// Triggered when the session has been refreshed.
    fn on_session_refresh(&self);
    /// Triggered when the session has been destroyed.
    fn on_session_deleted(&self);

    /// Triggered when the refresh operation fails.
    fn on_refresh_failed(&self, e: SessionError);

    /// Triggers if any error occurs while persisting the session data.
    fn on_error(&self, err: SessionError);
}

impl SessionError {
    fn from_core_session_err(value: &CoreSessionError) -> Self {
        match value {
            CoreSessionError::DB(err) => SessionError::DB(err.to_string()),
            CoreSessionError::Crypto => SessionError::Crypto,
            CoreSessionError::KeyChain(err) => SessionError::KeyChain(err.to_string()),
            CoreSessionError::KeyChainHasNoKey => SessionError::KeyChainHasNoKey,
            CoreSessionError::Other(err) => SessionError::Other(err.to_string()),
        }
    }

    fn from_http_err(value: &HttpRequestError) -> Self {
        SessionError::Http(value.to_string())
    }
}

pub(crate) struct FFISessionCallback(Box<dyn SessionCallback>);

impl From<Box<dyn SessionCallback>> for FFISessionCallback {
    fn from(value: Box<dyn SessionCallback>) -> Self {
        Self(value)
    }
}

impl CoreSessionCallback for FFISessionCallback {
    fn on_session_refresh(&self) {
        self.0.on_session_refresh()
    }

    fn on_session_deleted(&self) {
        self.0.on_session_deleted()
    }

    fn on_refresh_failed(&self, e: &HttpRequestError) {
        self.0.on_refresh_failed(SessionError::from_http_err(e))
    }

    fn on_error(&self, err: &CoreSessionError) {
        self.0.on_error(SessionError::from_core_session_err(err))
    }
}

/// Represents a session that has been stored on the device.
#[derive(uniffi::Object)]
pub struct StoredSession {
    session: EncryptedUserSession,
}

impl StoredSession {
    pub(crate) fn new(session: EncryptedUserSession) -> Arc<Self> {
        Arc::new(Self { session })
    }

    pub(crate) fn encrypted_session(&self) -> &EncryptedUserSession {
        &self.session
    }
}

#[uniffi::export]
impl StoredSession {
    /// Get the session's email.
    pub fn email(&self) -> String {
        self.session.email.clone()
    }

    /// Get the session's account name (if any).
    pub fn name(&self) -> Option<String> {
        self.session.name.clone()
    }

    /// Get the session's user id.
    pub fn user_id(&self) -> UserId {
        self.session.user_id.clone()
    }

    /// Get the session id.
    pub fn session_id(&self) -> Uid {
        self.session.session_id.clone()
    }
}
