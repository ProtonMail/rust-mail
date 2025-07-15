use anyhow::bail;
use async_trait::async_trait;
use muon::client::PasswordMode;
use muon::rest::auth::v4::fido2;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::{Auth, UserKeySecret};
use crate::services::proton::{SessionId, UserId};

/// A shared store.
pub type BoxStore = Box<dyn Store>;

/// A thread-safe, shared store.
pub type DynStore = Arc<RwLock<Box<dyn Store>>>;

/// The error type returned by the store.
pub type StoreError = anyhow::Error;

/// The info known about the user's authentication.
#[derive(Debug, Clone)]
pub struct AuthInfo {
    /// The user's ID.
    pub user_id: UserId,

    /// The session ID.
    pub session_id: SessionId,

    /// The 2FA mode.
    pub tfa_mode: TfaMode,

    /// The mailbox password mode.
    pub mbp_mode: MbpMode,

    /// TFA Fido details for a user.
    pub fido_details: Option<fido2::Response>,
}

/// The data known about the user.
#[derive(Debug, Clone)]
pub struct UserData {
    /// The name of the user.
    pub username: String,

    /// The user's display name.
    pub display_name: String,

    /// The user's primary email address.
    pub primary_addr: String,

    /// The user's key secret.
    pub key_secret: UserKeySecret,
}

#[must_use]
#[derive(Debug, Clone, Copy)]
pub struct TfaMode {
    /// Whether the user has TOTP enabled.
    pub totp: bool,

    /// Whether the user has FIDO2 enabled.
    pub fido: bool,
}

impl TfaMode {
    /// Create a new TFA mode with the given TOTP and FIDO2 settings.
    pub fn new(totp: bool, fido: bool) -> Self {
        Self { totp, fido }
    }

    /// Create a new TFA mode with no TOTP or FIDO2 enabled.
    pub fn none() -> Self {
        Self {
            totp: false,
            fido: false,
        }
    }
}

/// The mailbox password mode of a user's account.
#[derive(Debug, Clone, Copy)]
pub enum MbpMode {
    /// The user has only one password.
    One = 1,

    /// The user has two passwords.
    Two = 2,
}

impl From<PasswordMode> for MbpMode {
    fn from(mode: PasswordMode) -> Self {
        match mode {
            PasswordMode::One => Self::One,
            PasswordMode::Two => Self::Two,
        }
    }
}

/// Authentication storage abstraction trait in order to store or load auth data.
#[async_trait]
pub trait Store: Send + Sync + 'static {
    /// Set the name or address used to authenticate.
    fn set_name_or_addr(&mut self, name_or_addr: &str);

    /// Get the current auth session data.
    async fn get_auth(&self) -> Auth;

    /// Set the auth session data.
    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError>;

    /// Set the auth info.
    async fn set_auth_info(&mut self, info: AuthInfo) -> Result<(), StoreError>;

    /// Set the temporary encrypted username/password.
    async fn set_temp_pass(&mut self, pass: &str) -> Result<(), StoreError>;

    /// Set the user data.
    async fn set_user_data(&mut self, data: UserData) -> Result<(), StoreError>;

    /// Set the key secret.
    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError>;

    /// Get the user's key secret.
    async fn expose_key_secret(&self) -> Option<UserKeySecret>;

    /// Clear the temporary password.
    async fn clear_temp_pass(&mut self) -> Result<(), StoreError>;

    /// Clear all session data.
    async fn clear_session(&mut self) -> Result<(), StoreError>;

    /// Clear all account data.
    async fn clear_account(&mut self) -> Result<(), StoreError>;
}

#[async_trait]
impl<S: ?Sized + Store> Store for Box<S> {
    /// Set the name or address used to authenticate.
    fn set_name_or_addr(&mut self, name_or_addr: &str) {
        self.deref_mut().set_name_or_addr(name_or_addr);
    }

    async fn get_auth(&self) -> Auth {
        self.deref().get_auth().await
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError> {
        self.deref_mut().set_auth(auth).await
    }

    async fn set_auth_info(&mut self, info: AuthInfo) -> Result<(), StoreError> {
        self.deref_mut().set_auth_info(info).await
    }

    async fn set_temp_pass(&mut self, pass: &str) -> Result<(), StoreError> {
        self.deref_mut().set_temp_pass(pass).await
    }

    async fn set_user_data(&mut self, data: UserData) -> Result<(), StoreError> {
        self.deref_mut().set_user_data(data).await
    }

    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError> {
        self.deref_mut().set_key_secret(secret).await
    }

    async fn expose_key_secret(&self) -> Option<UserKeySecret> {
        self.deref().expose_key_secret().await
    }

    async fn clear_temp_pass(&mut self) -> Result<(), StoreError> {
        self.deref_mut().clear_temp_pass().await
    }

    async fn clear_session(&mut self) -> Result<(), StoreError> {
        self.deref_mut().clear_session().await
    }

    async fn clear_account(&mut self) -> Result<(), StoreError> {
        self.deref_mut().clear_account().await
    }
}

/// A dummy store implementation, used when no store is provided.
#[derive(Debug, Default)]
pub(crate) struct TempStore {
    auth: Auth,
    info: Option<AuthInfo>,
    data: Option<UserData>,
    name: Option<String>,
}

impl TempStore {
    pub fn boxed() -> Box<dyn Store> {
        Box::new(Self::default())
    }
}

#[async_trait]
impl Store for TempStore {
    fn set_name_or_addr(&mut self, name_or_addr: &str) {
        self.name = Some(name_or_addr.to_owned());
    }

    async fn get_auth(&self) -> Auth {
        self.auth.clone()
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError> {
        self.auth = auth;

        Ok(())
    }

    async fn set_auth_info(&mut self, info: AuthInfo) -> Result<(), StoreError> {
        self.info = Some(info);

        Ok(())
    }

    async fn set_temp_pass(&mut self, _: &str) -> Result<(), StoreError> {
        bail!("unsupported")
    }

    async fn set_user_data(&mut self, data: UserData) -> Result<(), StoreError> {
        self.data = Some(data);

        Ok(())
    }

    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError> {
        self.data.as_mut().unwrap().key_secret = secret;

        Ok(())
    }

    async fn expose_key_secret(&self) -> Option<UserKeySecret> {
        self.data.as_ref().map(|data| data.key_secret.clone())
    }

    async fn clear_temp_pass(&mut self) -> Result<(), StoreError> {
        bail!("unsupported")
    }

    async fn clear_session(&mut self) -> Result<(), StoreError> {
        *self = Self::default();

        Ok(())
    }

    async fn clear_account(&mut self) -> Result<(), StoreError> {
        *self = Self::default();

        Ok(())
    }
}
