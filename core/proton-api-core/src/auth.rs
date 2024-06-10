use crate::domain::{SecretString, Uid, UserId};
use crate::http::RequestError;
use proton_async::sync::RwLock;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Authentication scopes for the session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Scope(pub String);

/// Token used to refresh the active session.
#[derive(Deserialize, Debug, Clone)]
pub struct RefreshToken(pub SecretString);

/// Authentication token for the current session.
#[derive(Deserialize, Debug, Clone)]
pub struct AccessToken(pub SecretString);

/// Session Authentication Data.
#[derive(Clone)]
pub struct Auth {
    /// User email,
    pub email: String,
    /// User id,
    pub user_id: UserId,
    /// Session UID.
    pub uid: Uid,
    /// Refresh Token
    pub refresh_token: RefreshToken,
    /// Auth token
    pub access_token: AccessToken,
    /// Access scopes
    pub scope: Scope,
}

pub trait Store: Send + Sync + 'static {
    /// Get the current auth if any.
    fn get_auth(&self) -> Option<&Auth>;

    /// Set the new auth state.
    ///
    /// # Params
    /// * `auth`: New authentication state.
    ///
    /// # Errors
    /// Returns error if the update failed.
    fn set_auth(&mut self, auth: Auth) -> Result<(), Box<dyn std::error::Error>>;

    /// Notify the implementation that an attempt to refresh the credentials failed.
    ///
    /// # Params
    /// * `e`: Network error which occurred during refresh.
    fn refresh_auth_failed(&self, e: &RequestError);

    /// Called after the auth info has been updated via a session refresh.
    ///
    /// # Params
    /// * `uid`: session uid.
    /// * `access_token`: new access token.
    /// * `refresh_token`: new refresh token.
    /// * `scope`: new authentication scopes.
    ///
    /// # Errors
    /// Returns error if the new auth state could not be stored.
    fn refresh_auth(
        &mut self,
        uid: Uid,
        access_token: AccessToken,
        refresh_token: RefreshToken,
        scope: Scope,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Update the session authentication scope.
    ///
    /// # Errors
    /// Returns error if the auth state could not be updated.
    fn set_scopes(&mut self, scopes: Scope) -> Result<Option<&Auth>, Box<dyn std::error::Error>>;

    /// Clear the authentication state, will be called when the user logs out.
    ///
    /// # Errors
    /// Returns error if the auth state could not be deleted.
    fn clear_auth(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

/// Wrapper trait that tracks modifications to the auth data. Each write to the auth data should bump
/// the version counter. This is used to prevent concurrent refreshes.
pub trait VersionedAuthStore: Store {
    /// Get the current version counter.
    fn auth_refresh_version(&self) -> u32;
}

/// In memory authentication storage.
#[derive(Default)]
pub struct InMemoryStore {
    auth: Option<Auth>,
}

impl Store for InMemoryStore {
    fn get_auth(&self) -> Option<&Auth> {
        self.auth.as_ref()
    }

    fn set_auth(&mut self, auth: Auth) -> Result<(), Box<dyn std::error::Error>> {
        self.auth = Some(auth);
        Ok(())
    }

    fn set_scopes(&mut self, scope: Scope) -> Result<Option<&Auth>, Box<dyn std::error::Error>> {
        let Some(auth) = &mut self.auth else {
            return Ok(None);
        };

        auth.scope = scope;
        Ok(Some(auth))
    }

    fn refresh_auth(
        &mut self,
        uid: Uid,
        access_token: AccessToken,
        refresh_token: RefreshToken,
        scope: Scope,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(auth) = &mut self.auth {
            auth.uid = uid;
            auth.access_token = access_token;
            auth.refresh_token = refresh_token;
            auth.scope = scope;
        }
        Ok(())
    }

    fn clear_auth(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.auth = None;
        Ok(())
    }

    fn refresh_auth_failed(&self, _: &RequestError) {}
}
pub type ArcAuthStore = Arc<RwLock<dyn VersionedAuthStore>>;

pub fn new_arc_auth_store<T: Store>(auth: T) -> ArcAuthStore {
    Arc::new(RwLock::new(VersionedAuthStoreWrapper::new(auth)))
}

pub struct VersionedAuthStoreWrapper<T: Store> {
    version: u32,
    auth_store: T,
}

impl<T: Store> VersionedAuthStoreWrapper<T> {
    pub fn new(auth_store: T) -> Self {
        Self {
            auth_store,
            version: 0,
        }
    }
}

impl<T: Store> Store for VersionedAuthStoreWrapper<T> {
    fn get_auth(&self) -> Option<&Auth> {
        self.auth_store.get_auth()
    }

    fn set_auth(&mut self, auth: Auth) -> Result<(), Box<dyn std::error::Error>> {
        self.auth_store.set_auth(auth)?;
        Ok(())
    }

    fn refresh_auth_failed(&self, e: &RequestError) {
        self.auth_store.refresh_auth_failed(e);
    }

    fn refresh_auth(
        &mut self,
        uid: Uid,
        access_token: AccessToken,
        refresh_token: RefreshToken,
        scope: Scope,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.auth_store
            .refresh_auth(uid, access_token, refresh_token, scope)?;
        self.version = self.version.wrapping_add(1);
        Ok(())
    }

    fn set_scopes(&mut self, scopes: Scope) -> Result<Option<&Auth>, Box<dyn std::error::Error>> {
        self.auth_store.set_scopes(scopes)
    }

    fn clear_auth(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.auth_store.clear_auth()?;
        Ok(())
    }
}

impl<T: Store> VersionedAuthStore for VersionedAuthStoreWrapper<T> {
    fn auth_refresh_version(&self) -> u32 {
        self.version
    }
}

impl AccessToken {
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl RefreshToken {
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}
impl<T: Into<String>> From<T> for AccessToken {
    fn from(value: T) -> Self {
        Self(SecretString::new(value.into()))
    }
}

impl<T: Into<String>> From<T> for RefreshToken {
    fn from(value: T) -> Self {
        Self(SecretString::new(value.into()))
    }
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for Scope {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

#[cfg(feature = "sql")]
impl stash::exports::ToSql for Scope {
    fn to_sql(&self) -> Result<stash::exports::ToSqlOutput<'_>, stash::exports::SqliteError> {
        self.0.to_sql()
    }
}

#[cfg(feature = "sql")]
impl stash::exports::FromSql for Scope {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        String::column_result(value).map(Self)
    }
}
