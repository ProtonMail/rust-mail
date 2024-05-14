use crate::core::{FFIKeyChain, FFINetworkStatusChanged, NetworkStatusChanged};
use crate::core::{FFISessionCallback, OSKeyChain, SessionCallback, StoredSession};
use crate::mail::logging::init_log;
use crate::mail::settings::{FFIMailsSettingsCallback, MailSettingsUpdated};
use crate::mail::{LoginFlow, MailUserSession};
use pmc::db;
use pmc::db::DBMigrationError;
use pmc::exports::proton_event_loop::EventLoopError;
use pmc::exports::{anyhow, thiserror, tracing};
use pmc::proton_api_mail::proton_api_core::http::{APIEnvConfig, RequestError};
use pmc::proton_core_common::db::SessionEncryptionKey;
use proton_mail_common as pmc;
use proton_mail_common::exports::anyhow::anyhow;
use proton_mail_common::proton_api_mail::domain::AddressDomainLogoError;
use proton_mail_common::proton_api_mail::proton_api_core::http;
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
pub struct MailSession {
    ctx: pmc::MailContext,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailSessionError {
    #[error("Database Error: {0}")]
    DB(#[from] db::DBError),
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
    Http(#[from] RequestError),
    #[error("Event Loop: {0}")]
    EventLoop(#[from] EventLoopError),
    #[error("Action Queue: {0}")]
    ActionQueue(#[from] proton_mail_common::exports::proton_action_queue::QueueError),
    #[error("Failed to access PGP keys: {0}")]
    PGPKeyAccess(anyhow::Error),
    #[error("Invalid mode: '{0}'")]
    InvalidImageMode(String),
    #[error("Creating AddressDomainLogoDetails failed with error: '{0}'")]
    AddressDomainLogoError(#[from] AddressDomainLogoError),
    #[error("{0}")]
    Other(anyhow::Error),
}
pub type MailSessionResult<T> = Result<T, MailSessionError>;

#[uniffi::export]
impl MailSession {
    /// Create a new mail context:
    /// * `session_dir`: Directory where the session db should be stored.
    /// * `user_dri`: Directory where the user db should be stored.
    /// * `log_dir:`: Directory where the log file should be stored.
    /// * `log_debug`: Whether to enable debug and trace logs
    /// * `api_env_config`: The API environment configuration.
    /// * `key_chain`: `KeyChain` implementation
    /// * `network_callback`: Optional network status changed callback
    #[uniffi::constructor]
    pub fn create(
        session_dir: String,
        user_dir: String,
        log_dir: String,
        log_debug: bool,
        key_chain: Box<dyn OSKeyChain>,
        api_env_config: Option<APIEnvConfig>,
        network_callback: Option<Box<dyn NetworkStatusChanged>>,
    ) -> MailSessionResult<Self> {
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
            .map_err(|e| MailSessionError::KeyChain(e.to_string()))?
            .is_none()
        {
            tracing::debug!("Key chain has no key, generating");
            let key = SessionEncryptionKey::random();
            key_chain.store(key.to_base64()).map_err(|e| {
                tracing::error!("Failed to store key in keychain");
                MailSessionError::KeyChain(e.to_string())
            })?;
        }

        // Creating runtime.
        let runtime = proton_async::runtime::MultiThreaded::new(4).map_err(|e| {
            MailSessionError::Other(anyhow::anyhow!("Failed to init async runtime: {e}"))
        })?;

        // Creating client.
        let api_env_config = match api_env_config {
            Some(config) => config,
            None => APIEnvConfig::default(),
        };

        let mut client = http::Builder::new().api_env_config(api_env_config);

        if session_debug_enabled() {
            client = client.debug();
        }

        let client = client.build().map_err(|e| {
            MailSessionError::Http(RequestError::Other(anyhow!("Failed to create client: {e}")))
        })?;

        tracing::debug!("Creating Context");
        let mail_ctx = pmc::MailContext::new(
            runtime,
            session_path,
            user_path,
            Arc::from(FFIKeyChain::from(key_chain)),
            client,
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
    ) -> MailSessionResult<Arc<LoginFlow>> {
        let flow = self
            .ctx
            .new_login_flow(cb.map(|cb| -> Box<dyn CoreSessionCallback> {
                Box::new(FFISessionCallback::from(cb))
            }))?;
        Ok(LoginFlow::new(flow, self.ctx.clone()))
    }

    /// Retrieve the currently stored sessions.
    pub fn stored_sessions(&self) -> MailSessionResult<Vec<Arc<StoredSession>>> {
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
        session_cb: Option<Box<dyn SessionCallback>>,
        mail_settings_cb: Option<Box<dyn MailSettingsUpdated>>,
    ) -> MailSessionResult<Arc<MailUserSession>> {
        let session_cb = session_cb
            .map(|cb| -> Box<dyn CoreSessionCallback> { Box::new(FFISessionCallback::from(cb)) });
        let ctx = self.ctx.user_context_from_session(
            session.encrypted_session(),
            session_cb,
            mail_settings_cb.map(FFIMailsSettingsCallback::boxed),
        )?;
        Ok(MailUserSession::new(ctx))
    }

    /// Check whether the network is connected/online.
    #[must_use]
    pub fn is_network_connected(&self) -> bool {
        self.ctx.is_network_connected()
    }

    /// Externally notify the context that the network connection has changed.
    pub fn set_network_connected(&self, online: bool) {
        self.ctx.set_network_connected(online);
    }
}

impl From<pmc::MailContextError> for MailSessionError {
    fn from(value: proton_mail_common::MailContextError) -> Self {
        match value {
            pmc::MailContextError::DB(v) => MailSessionError::DB(v),
            pmc::MailContextError::Crypto => MailSessionError::Crypto,
            pmc::MailContextError::KeyChain(k) => MailSessionError::KeyChain(k.to_string()),
            pmc::MailContextError::IO(io) => MailSessionError::IO(io),
            pmc::MailContextError::DBMigration(err) => MailSessionError::DBMigration(err),
            pmc::MailContextError::KeyChainHasNoKey => MailSessionError::KeyChainHasNoKey,
            pmc::MailContextError::Http(err) => MailSessionError::Http(err),
            pmc::MailContextError::EventLoop(err) => MailSessionError::EventLoop(err),
            pmc::MailContextError::Other(err) => MailSessionError::Other(err),
            pmc::MailContextError::ActionQueue(e) => Self::ActionQueue(e),
            pmc::MailContextError::PGPKeyAccess(e) => Self::PGPKeyAccess(anyhow!("{e}")),
            pmc::MailContextError::AddressDomainLogoError(e) => Self::AddressDomainLogoError(e),
        }
    }
}

fn session_debug_enabled() -> bool {
    std::env::var("PROTON_CORE_CTX_SESSION_DEBUG").is_ok()
}
