#![allow(clippy::module_name_repetitions)]

use crate::services::proton::common::RemoteId;
use crate::services::proton::response_data::{PasswordMode, TfaStatus, User};
use crate::services::proton::responses::PostAuthResponse;
use crate::X_PM_UID_HEADER;
use futures::future::BoxFuture;
use parking_lot::RwLock;
use proton_crypto_account::salts::KeySecret;
use reqwest::header::{HeaderMap, HeaderValue};
pub use secrecy::{ExposeSecret, SecretString as RealSecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::ops::Deref;
use std::sync::Arc;

/// Authentication session data.
///
/// This type holds data pertaining to a single authenticated API session:
/// - The UID of the session (uniquely identifies the session across the API),
/// - The ID of the user who is authenticated,
/// - The access token granted by the API to authenticate requests,
/// - The refresh token used to refresh the access token when it expires,
/// - The API scopes the tokens grant access to,
/// - The state of the auth session (whether a second factor is required, etc).
///
/// Notably, this type does not hold the user's key secret, as this is not tied
/// to a specific session; instead, it is stored in the [`AuthSecrets`] type.
#[derive(Clone)]
pub struct AuthSession {
    /// The UID of the current session.
    pub uid: RemoteId,

    /// The remote ID of the current user.
    pub user_id: RemoteId,

    /// The name or address of the user, whatever was used to authenticate.
    pub name_or_addr: String,

    /// The second factor mode used by the account when the session was created.
    pub second_factor_mode: TfaStatus,

    /// The password mode used by the account when the session was created.
    pub password_mode: PasswordMode,

    /// The access token for the current session.
    pub access_token: SecretString,

    /// The refresh token, used to refresh the access token.
    pub refresh_token: SecretString,

    /// The API scopes to which the tokens grant access.
    pub auth_scope: Vec<String>,

    /// The state of the auth session.
    pub auth_state: AuthState,
}

impl AuthSession {
    #[must_use]
    pub fn from_response(name_or_addr: String, res: PostAuthResponse) -> Self {
        let auth_state = if res.tfa.enabled == TfaStatus::None {
            AuthState::Ready
        } else {
            AuthState::TwoFA
        };

        Self {
            uid: res.uid,
            name_or_addr,
            user_id: res.user_id,
            second_factor_mode: res.tfa.enabled,
            password_mode: res.password_mode,
            access_token: res.access_token,
            refresh_token: res.refresh_token,
            auth_scope: res.scopes,
            auth_state,
        }
    }
}

/// Secrets associated with a user.
///
/// This type holds the user's key secret, which is used to unlock the user's PGP key(s).
/// Unlike the [`AuthSession`] type, this type is not tied to a specific session.
#[derive(Clone)]
pub struct UserSecrets {
    /// The secret used to unlock user keys.
    pub key_secret: UserKeySecret,
}

impl UserSecrets {
    #[must_use]
    pub fn new(key_secret: UserKeySecret) -> Self {
        Self { key_secret }
    }
}

/// Information related to a user's account.
///
/// TODO: Decide which fields really need to be optional.
#[derive(Clone)]
pub struct AccountInfo {
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub primary_addr: Option<String>,
}

impl AccountInfo {
    #[must_use]
    pub fn from_user(user: User) -> Self {
        Self {
            username: user.name,
            display_name: user.display_name,
            primary_addr: Some(user.email),
        }
    }
}

/// The state of the auth (whether a second factor must still be completed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    /// The auth session requires 2FA to be completed.
    TwoFA,

    /// The auth session is ready to be used.
    Ready,
}

/// TODO: Document this struct.
#[derive(Debug, Clone)]
pub struct SecretString(RealSecretString);

impl Deref for SecretString {
    type Target = RealSecretString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SecretString(RealSecretString::deserialize(deserializer)?))
    }
}

impl Eq for SecretString {}

impl From<RealSecretString> for SecretString {
    fn from(value: RealSecretString) -> Self {
        Self(value)
    }
}

impl From<SecretString> for RealSecretString {
    fn from(value: SecretString) -> Self {
        value.0
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(RealSecretString::new(value))
    }
}

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret() == other.0.expose_secret()
    }
}

impl Serialize for SecretString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("[redacted]")
    }
}

/// The user key secret to unlock user keys.
#[derive(Debug, Clone)]
pub struct UserKeySecret(pub KeySecret);

impl UserKeySecret {
    /// Exposes the internal key secret to unlock user keys.
    #[must_use]
    pub fn expose_secret(&self) -> &KeySecret {
        &self.0
    }
}

impl<T: Into<Vec<u8>>> From<T> for UserKeySecret {
    fn from(value: T) -> Self {
        Self(KeySecret::new(value.into()))
    }
}

pub type StoreError = Box<dyn Error + Send + Sync>;

/// Authentication storage abstraction trait in order to store or load auth data.
pub trait Store: Send + Sync {
    /// Retrieve the auth session data from the store.
    ///
    /// If no value exists, return `None`.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn get_auth_session(&self) -> BoxFuture<'_, Result<Option<AuthSession>, StoreError>>;

    /// Retrieve the secrets from the store.
    ///
    /// If no value exists, return `None`.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn get_user_secrets(&self) -> BoxFuture<'_, Result<Option<UserSecrets>, StoreError>>;

    /// Retrieve the account info from the store.
    ///
    /// If no value exists, return `None`.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn get_account_info(&self) -> BoxFuture<'_, Result<Option<AccountInfo>, StoreError>>;

    /// Update the auth session data in the store.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn set_auth_session(&mut self, auth: AuthSession) -> BoxFuture<'_, Result<(), StoreError>>;

    /// Update the secrets in the store.
    ///
    /// If no value exists, one should be created.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn set_user_secrets(&mut self, secrets: UserSecrets) -> BoxFuture<'_, Result<(), StoreError>>;

    /// Update the account info in the store.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn set_account_info(&mut self, info: AccountInfo) -> BoxFuture<'_, Result<(), StoreError>>;

    /// Remove the auth data from the store.
    ///
    /// Returns the previous value, if any.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn clear(&mut self) -> BoxFuture<'_, Result<(), StoreError>>;
}

/// In memory cache of the auth data which can optionally be backed by an authentication [`Store`].
pub(crate) struct CachedStore {
    headers: Arc<RwLock<HeaderMap>>,

    auth_session: Option<AuthSession>,
    user_secrets: Option<UserSecrets>,
    account_info: Option<AccountInfo>,

    store: Option<Box<dyn Store>>,
}

impl CachedStore {
    /// Creates a new instance which reads the currently store value in to memory.
    ///
    /// If no `store` is provided, this type acts as a pure in-memory cache, otherwise
    /// the data is read and stored into the `store` as required.
    ///
    /// # Errors
    ///
    /// Returns error if we can't read from the store.
    pub(crate) async fn new(
        store: Option<Box<dyn Store>>,
        headers: Arc<RwLock<HeaderMap>>,
    ) -> Result<Self, StoreError> {
        let (auth_session, user_secrets, account_info) = if let Some(store) = &store {
            (
                store.get_auth_session().await?,
                store.get_user_secrets().await?,
                store.get_account_info().await?,
            )
        } else {
            (None, None, None)
        };

        if let Some(auth) = &auth_session {
            update_auth_headers(&headers, auth)?;
        }

        Ok(Self {
            headers,
            auth_session,
            user_secrets,
            account_info,
            store,
        })
    }

    /// Get the auth session data, if available.
    pub(crate) fn get_auth_session(&self) -> Option<&AuthSession> {
        self.auth_session.as_ref()
    }

    /// Get the auth secrets, if available.
    pub(crate) fn get_user_secrets(&self) -> Option<&UserSecrets> {
        self.user_secrets.as_ref()
    }

    /// Get the account info, if available.
    #[allow(unused)]
    pub(crate) fn get_account_info(&self) -> Option<&AccountInfo> {
        self.account_info.as_ref()
    }

    /// Update the auth session data.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be stored.
    pub(crate) async fn set_auth_session(&mut self, auth: AuthSession) -> Result<(), StoreError> {
        update_auth_headers(&self.headers, &auth)?;

        if let Some(store) = &mut self.store {
            store.set_auth_session(auth.clone()).await?;
        }

        self.auth_session = Some(auth);

        Ok(())
    }

    /// Update the auth secrets.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be stored.
    pub(crate) async fn set_user_secrets(&mut self, data: UserSecrets) -> Result<(), StoreError> {
        if let Some(store) = &mut self.store {
            store.set_user_secrets(data.clone()).await?;
        }

        self.user_secrets = Some(data);

        Ok(())
    }

    /// Update the account info.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be stored.
    pub(crate) async fn set_account_info(&mut self, info: AccountInfo) -> Result<(), StoreError> {
        if let Some(store) = &mut self.store {
            store.set_account_info(info.clone()).await?;
        }

        self.account_info = Some(info);

        Ok(())
    }

    /// Clear the auth data.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not cleared.
    pub(crate) async fn clear(
        &mut self,
    ) -> Result<(Option<AuthSession>, Option<UserSecrets>), StoreError> {
        remove_auth_headers(&self.headers);

        if let Some(store) = &mut self.store {
            store.clear().await?;
        }

        Ok((self.auth_session.take(), self.user_secrets.take()))
    }
}

fn update_auth_headers(
    header_map: &Arc<RwLock<HeaderMap>>,
    auth: &AuthSession,
) -> Result<(), StoreError> {
    let mut guard = header_map.write();
    guard.insert(
        X_PM_UID_HEADER,
        HeaderValue::from_str(auth.uid.as_ref()).map_err(Box::new)?,
    );
    guard.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", auth.access_token.expose_secret()))
            .map_err(Box::new)?,
    );

    Ok(())
}

fn remove_auth_headers(header_map: &Arc<RwLock<HeaderMap>>) {
    let mut guard = header_map.write();
    guard.remove(X_PM_UID_HEADER);
    guard.remove("Authorization");
}
