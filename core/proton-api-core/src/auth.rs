use crate::services::proton::common::RemoteId;
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
#[derive(Clone)]
pub struct Auth {
    /// The authentication token for the current session.
    pub access_token: SecretString,

    /// The name or address of the user, whatever was used to authenticate.
    pub name_or_addr: String,

    /// A [`KeySecret`] to unlock the user's keys.
    pub key_secret: Option<UserKeySecret>,

    /// TODO: Document this field.
    pub refresh_token: SecretString,

    /// TODO: Document this field.
    pub scopes: Vec<String>,

    /// The UID of the current session.
    pub uid: RemoteId,

    /// The remote ID of the current user.
    pub user_id: RemoteId,
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

/// Authentication storage abstraction trait in order to store or load [`Auth`] data.
pub trait Store: Send + Sync {
    /// Update the `auth` value in the store.
    ///
    /// If no value exists, one should be created.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn set(&mut self, auth: Auth) -> BoxFuture<'_, Result<(), StoreError>>;

    /// Retrieve the auth data from the store.
    ///
    /// If no value exists, return `None`.
    ///
    /// # Errors
    ///
    /// Returns error if the operation failed.
    fn get(&self) -> BoxFuture<'_, Result<Option<Auth>, StoreError>>;

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
    cached: Option<Auth>,
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
        let auth = if let Some(store) = &store {
            store.get().await?
        } else {
            None
        };

        if let Some(auth) = &auth {
            update_auth_headers(&headers, auth)?;
        }

        Ok(Self {
            cached: auth,
            store,
            headers,
        })
    }

    /// Get the auth data, if available.
    pub(crate) fn get(&self) -> Option<&Auth> {
        self.cached.as_ref()
    }

    /// Update the auth data.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not be stored.
    pub(crate) async fn set(&mut self, auth: Auth) -> Result<(), StoreError> {
        // Update auth headers.
        update_auth_headers(&self.headers, &auth)?;
        if let Some(store) = &mut self.store {
            store.set(auth.clone()).await?;
        }
        self.cached = Some(auth);
        Ok(())
    }

    /// Update the scopes of the auth data.
    ///
    /// # Errors
    ///
    /// Returns error if no auth is present or the data could not be stored.
    pub(crate) async fn set_scopes(&mut self, scopes: Vec<String>) -> Result<(), StoreError> {
        let Some(auth) = self.cached.as_mut() else {
            return Err("No auth data available for scope update")?;
        };

        if let Some(store) = &mut self.store {
            store.set(auth.to_owned()).await?;
        }

        auth.scopes = scopes;

        Ok(())
    }

    /// Clear the auth data.
    ///
    /// # Errors
    ///
    /// Returns error if the data could not cleared.
    pub(crate) async fn clear(&mut self) -> Result<Option<Auth>, StoreError> {
        // Remove auth headers.
        remove_auth_headers(&self.headers);
        if let Some(store) = &mut self.store {
            store.clear().await?;
        }
        Ok(self.cached.take())
    }
}

fn update_auth_headers(header_map: &Arc<RwLock<HeaderMap>>, auth: &Auth) -> Result<(), StoreError> {
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
