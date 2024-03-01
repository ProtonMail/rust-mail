use crate::domain::{SecretString, Uid, UserId};
use parking_lot::RwLock;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
pub struct AuthScope(String);

impl AsRef<str> for AuthScope {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for AuthScope {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

#[cfg(feature = "sql")]
impl proton_sqlite3::rusqlite::types::ToSql for AuthScope {
    fn to_sql(
        &self,
    ) -> proton_sqlite3::rusqlite::Result<proton_sqlite3::rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[cfg(feature = "sql")]
impl proton_sqlite3::rusqlite::types::FromSql for AuthScope {
    fn column_result(
        value: proton_sqlite3::rusqlite::types::ValueRef<'_>,
    ) -> proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
        String::column_result(value).map(Self)
    }
}

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
    pub refresh_token: SecretString,
    /// Auth token
    pub access_token: SecretString,
    /// Access scopes
    pub scope: AuthScope,
}

pub trait AuthStore: Send + Sync + 'static {
    /// Get the current auth if any.
    fn get_auth(&self) -> Option<&Auth>;
    fn set_auth(&mut self, auth: Auth) -> Result<(), Box<dyn std::error::Error>>;

    fn refresh_auth(
        &mut self,
        uid: Uid,
        access_token: SecretString,
        refresh_token: SecretString,
        scope: AuthScope,
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn set_scopes(
        &mut self,
        scopes: AuthScope,
    ) -> Result<Option<&Auth>, Box<dyn std::error::Error>>;
    fn clear_auth(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

/// In memory authentication storage.

#[derive(Default)]
pub struct InMemoryStore {
    auth: Option<Auth>,
}

impl AuthStore for InMemoryStore {
    fn get_auth(&self) -> Option<&Auth> {
        self.auth.as_ref()
    }

    fn set_auth(&mut self, auth: Auth) -> Result<(), Box<dyn std::error::Error>> {
        self.auth = Some(auth);
        Ok(())
    }

    fn set_scopes(
        &mut self,
        scope: AuthScope,
    ) -> Result<Option<&Auth>, Box<dyn std::error::Error>> {
        let Some(auth) = &mut self.auth else {
            return Ok(None);
        };

        auth.scope = scope;
        Ok(Some(auth))
    }

    fn refresh_auth(
        &mut self,
        uid: Uid,
        access_token: SecretString,
        refresh_token: SecretString,
        scope: AuthScope,
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
}
pub type ArcAuthStore = Arc<RwLock<dyn AuthStore>>;

pub fn new_arc_auth_store<T: AuthStore>(auth: T) -> ArcAuthStore {
    Arc::new(RwLock::new(auth))
}
