use async_trait::async_trait;
use muon::client::PasswordMode;
use proton_crypto_account::salts::KeySecret;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A shared store.
pub type BoxStore = Box<dyn Store>;

/// A thread-safe, shared store.
pub type DynStore = Arc<RwLock<Box<dyn Store>>>;

/// The info known about the user's authentication.
///
/// TODO: Remove code duplication with `core/core-api/src/store.rs`
#[derive(Debug, Clone)]
pub struct AuthInfo {
    /// The user's ID.
    pub user_id: String,

    /// The session ID.
    pub session_id: String,

    /// The mailbox password mode.
    pub password_mode: PasswordMode,
}

/// The data known about the user.
///
/// TODO: Remove code duplication with `core/core-api/src/store.rs`
#[derive(Debug, Clone)]
pub struct UserData {
    /// The name of the user.
    pub username: String,

    /// The user's display name.
    pub display_name: String,

    /// The user's primary email address.
    pub primary_addr: String,

    /// The user's key secret.
    pub key_secret: KeySecret,
}

/// Authentication storage abstraction trait in order to store or load auth data.
///
/// TODO: Remove code duplication with `core/core-api/src/store.rs`
#[async_trait]
pub trait Store: Send + Sync + 'static {
    /// Set the name or address used to authenticate.
    async fn set_name_or_addr(&mut self, name_or_addr: &str);

    /// Set the auth info.
    async fn set_auth_info(&mut self, info: AuthInfo) -> Result<(), String>;

    /// Set the user data.
    async fn set_user_data(&mut self, data: UserData) -> Result<(), String>;
}
