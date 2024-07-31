use crate::core::datatypes::RemoteId;
use proton_mail_common::proton_core_common::db::session::EncryptedUserSession;
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
    #[must_use]
    pub fn email(&self) -> String {
        self.session.email.clone()
    }

    /// Get the session's account name (if any).
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.session.name.clone()
    }

    /// Get the session's user id.
    #[must_use]
    pub fn user_id(&self) -> RemoteId {
        self.session.user_id.clone().into()
    }

    /// Get the session id.
    #[must_use]
    pub fn session_id(&self) -> RemoteId {
        self.session.session_id.clone().into()
    }
}
