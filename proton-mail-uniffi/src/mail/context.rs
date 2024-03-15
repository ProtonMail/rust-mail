use crate::core::{FFIKeyChain, FFINetworkStatusChanged, NetworkStatusChanged};
use crate::core::{FFISessionCallback, OSKeyChain, SessionCallback, StoredSession};
use crate::mail::logging::init_log;
use crate::mail::{LoginFlow, MailUserContext};
use pmc::exports::proton_event_loop::EventLoopError;
use pmc::exports::{anyhow, thiserror, tracing};
use pmc::proton_api_mail::proton_api_core::http::HttpRequestError;
use pmc::proton_core_common::proton_core_db::SessionEncryptionKey;
use pmc::proton_mail_db;
use pmc::proton_mail_db::DBMigrationError;
use proton_mail_common as pmc;
use proton_mail_common::proton_core_common::CoreSessionCallback;
use std::path::PathBuf;
use std::sync::Arc;

/// Mail context is the entry point for the application. It contains important state such as
/// database connection pools and the async runtime for rust.
///
/// # Lifetime
/// This object needs to be kept alive for the entire duration of the application.
///
#[derive(uniffi::Object)]
pub struct MailContext {
    ctx: pmc::MailContext,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailContextError {
    #[error("Database Error: {0}")]
    DB(#[from] proton_mail_db::DBError),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(String),
    #[error("IO Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Database Migration Error: {0}")]
    DBMigration(#[from] DBMigrationError),
    #[error("No session key is available in the keychain")]
    KeyChainHasNoKey,
    #[error("HTTP Error: {0}")]
    Http(#[from] HttpRequestError),
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("{0}")]
    Other(anyhow::Error),
}
pub type MailContextResult<T> = Result<T, MailContextError>;

#[uniffi::export]
impl MailContext {
    /// Create a new mail context:
    /// * `session_dir`: Directory where the session db should be stored.
    /// * `user_dri`: Directory where the user db should be stored.
    /// * `log_dir:`: Directory where the log file should be stored.
    /// * `log_debug`: Whether to enable debug and trace logs
    /// * `key_chain`: KeyChain implementation
    /// * `network_callback`: Optional network status changed callback
    #[uniffi::constructor]
    pub fn new(
        session_dir: String,
        user_dir: String,
        log_dir: String,
        log_debug: bool,
        key_chain: Box<dyn OSKeyChain>,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> MailContextResult<Self> {
        let mut log_path = PathBuf::from(log_dir);
        std::fs::create_dir_all(&log_path)?;
        log_path.push("proton-mail-uniffi.log");

        init_log(&log_path, log_debug)?;

        let session_path = PathBuf::from(session_dir);
        let user_path = PathBuf::from(user_dir);

        // create directories.
        tracing::debug!("Creating directories");
        std::fs::create_dir_all(&session_path)?;
        std::fs::create_dir_all(&user_path)?;

        // Generate session key;
        tracing::debug!("Checking keychain");
        if key_chain
            .get()
            .map_err(|e| MailContextError::KeyChain(e.to_string()))?
            .is_none()
        {
            tracing::debug!("Key chain has no key, generating");
            let key = SessionEncryptionKey::random();
            key_chain.store(key.to_base64()).map_err(|e| {
                tracing::error!("Failed to store key in keychain");
                MailContextError::KeyChain(e.to_string())
            })?;
        }

        // Creating runtime.
        let runtime = proton_async::runtime::MTRuntime::new(4).map_err(|e| {
            MailContextError::Other(anyhow::anyhow!("Failed to init async runtime: {e}"))
        })?;

        tracing::debug!("Creating Context");
        let mail_ctx = pmc::MailContext::new(
            runtime,
            session_path,
            user_path,
            Arc::from(FFIKeyChain::from(key_chain)),
            network_callback.map(
                |v| -> Box<dyn proton_mail_common::proton_core_common::NetworkStatusChanged> {
                    Box::new(FFINetworkStatusChanged::from(v))
                },
            ),
        )?;
        Ok(Self { ctx: mail_ctx })
    }

    /// Start new login flow.
    pub fn new_login_flow(
        &self,
        cb: Option<Box<dyn SessionCallback>>,
    ) -> MailContextResult<Arc<LoginFlow>> {
        let flow = self
            .ctx
            .new_login_flow(cb.map(|cb| -> Box<dyn CoreSessionCallback> {
                Box::new(FFISessionCallback::from(cb))
            }))?;
        Ok(LoginFlow::new(flow, self.ctx.clone()))
    }

    /// Retrieve the currently stored sessions.
    pub fn stored_sessions(&self) -> MailContextResult<Vec<Arc<StoredSession>>> {
        let sessions = self.ctx.get_sessions()?;
        Ok(sessions
            .into_iter()
            .map(StoredSession::new)
            .collect::<Vec<_>>())
    }

    /// Create an user context from a stored session.
    pub fn user_context_from_session(
        &self,
        session: &StoredSession,
        cb: Option<Box<dyn SessionCallback>>,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let cb =
            cb.map(|cb| -> Box<dyn CoreSessionCallback> { Box::new(FFISessionCallback::from(cb)) });
        let ctx = self
            .ctx
            .user_context_from_session(session.encrypted_session(), cb)?;
        Ok(MailUserContext::new(ctx))
    }

    /// Check whether the network is connected/online.
    pub fn is_network_connected(&self) -> bool {
        self.ctx.is_network_connected()
    }

    /// Externally notify the context that the network connection has changed.
    pub fn set_network_connected(&self, online: bool) {
        self.ctx.set_network_connected(online);
    }
}

impl From<pmc::MailContextError> for MailContextError {
    fn from(value: proton_mail_common::MailContextError) -> Self {
        match value {
            pmc::MailContextError::DB(v) => MailContextError::DB(v),
            pmc::MailContextError::Crypto => MailContextError::Crypto,
            pmc::MailContextError::KeyChain(k) => MailContextError::KeyChain(k.to_string()),
            pmc::MailContextError::IO(io) => MailContextError::IO(io),
            pmc::MailContextError::DBMigration(err) => MailContextError::DBMigration(err),
            pmc::MailContextError::KeyChainHasNoKey => MailContextError::KeyChainHasNoKey,
            pmc::MailContextError::Http(err) => MailContextError::Http(err),
            pmc::MailContextError::EventLoop(err) => MailContextError::EventLoop(err),
            pmc::MailContextError::Other(err) => MailContextError::Other(err),
        }
    }
}
