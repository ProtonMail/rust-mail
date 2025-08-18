use anyhow::bail;
use async_trait::async_trait;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::{Auth, UserKeySecret};
use crate::services::proton::{PasswordMode, SessionId, UserId};

pub type BoxStore = Box<dyn Store>;
pub type DynStore = Arc<RwLock<Box<dyn Store>>>;
pub type StoreError = anyhow::Error;

#[derive(Debug, Clone)]
pub struct AuthInfo {
    pub user_id: UserId,
    pub session_id: SessionId,
    pub tfa_mode: TfaMode,
}

#[derive(Debug, Clone)]
pub struct UserData {
    pub username: String,
    pub display_name: String,
    pub primary_addr: String,
    pub password_mode: MbpMode,
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
    pub fn new(totp: bool, fido: bool) -> Self {
        Self { totp, fido }
    }

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

impl From<muon::client::PasswordMode> for MbpMode {
    fn from(mode: muon::client::PasswordMode) -> Self {
        match mode {
            muon::client::PasswordMode::One => Self::One,
            muon::client::PasswordMode::Two => Self::Two,
        }
    }
}

impl From<PasswordMode> for MbpMode {
    fn from(mode: PasswordMode) -> Self {
        match mode {
            PasswordMode::One => Self::One,
            PasswordMode::Two => Self::Two,
        }
    }
}

#[async_trait]
pub trait Store: Send + Sync + 'static {
    fn set_name_or_addr(&mut self, name_or_addr: &str);
    async fn get_auth(&self) -> Auth;
    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError>;
    async fn set_auth_info(&mut self, info: AuthInfo) -> Result<(), StoreError>;

    /// Note that `pass` is encrypted here
    async fn set_pass(&mut self, pass: &str) -> Result<(), StoreError>;
    async fn clear_pass(&mut self) -> Result<(), StoreError>;

    /// Set the temporary password flag.
    async fn set_temp_pass(&mut self, value: bool) -> Result<(), StoreError>;

    async fn set_user_data(&mut self, data: UserData) -> Result<(), StoreError>;
    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError>;
    async fn expose_key_secret(&self) -> Option<UserKeySecret>;
    async fn clear_session(&mut self) -> Result<(), StoreError>;
    async fn clear_account(&mut self) -> Result<(), StoreError>;
    async fn get_session_id(&self, user_id: &UserId) -> Result<Option<SessionId>, StoreError>;
}

#[async_trait]
impl<S: ?Sized + Store> Store for Box<S> {
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

    async fn set_pass(&mut self, pass: &str) -> Result<(), StoreError> {
        self.deref_mut().set_pass(pass).await
    }

    async fn clear_pass(&mut self) -> Result<(), StoreError> {
        self.deref_mut().clear_pass().await
    }

    async fn set_temp_pass(&mut self, value: bool) -> Result<(), StoreError> {
        self.deref_mut().set_temp_pass(value).await
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

    async fn clear_session(&mut self) -> Result<(), StoreError> {
        self.deref_mut().clear_session().await
    }

    async fn clear_account(&mut self) -> Result<(), StoreError> {
        self.deref_mut().clear_account().await
    }

    async fn get_session_id(&self, user_id: &UserId) -> Result<Option<SessionId>, StoreError> {
        self.deref().get_session_id(user_id).await
    }
}

/// A dummy store implementation, used when no store is provided.
#[derive(Debug, Default)]
pub struct TempStore {
    auth: Auth,
    info: Option<AuthInfo>,
    data: Option<UserData>,
    name: Option<String>,
}

impl TempStore {
    #[must_use]
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

    async fn set_pass(&mut self, _: &str) -> Result<(), StoreError> {
        bail!("unsupported")
    }

    async fn clear_pass(&mut self) -> Result<(), StoreError> {
        bail!("unsupported")
    }

    async fn set_temp_pass(&mut self, _: bool) -> Result<(), StoreError> {
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

    async fn clear_session(&mut self) -> Result<(), StoreError> {
        *self = Self::default();

        Ok(())
    }

    async fn clear_account(&mut self) -> Result<(), StoreError> {
        *self = Self::default();

        Ok(())
    }

    async fn get_session_id(&self, user_id: &UserId) -> Result<Option<SessionId>, StoreError> {
        let Some(info) = &self.info else {
            return Ok(None);
        };

        if &info.user_id != user_id {
            return Ok(None);
        }

        Ok(Some(info.session_id.clone()))
    }
}
