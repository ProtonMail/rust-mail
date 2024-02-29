use crate::domain::{SecretString, Uid};
use parking_lot::RwLock;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
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

/// Session Authentication Data.
#[derive(Clone)]
pub struct Auth {
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
    fn set_auth(
        &mut self,
        uid: Uid,
        refresh_token: SecretString,
        access_token: SecretString,
        scopes: AuthScope,
    );
    fn set_scopes(&mut self, scopes: AuthScope) -> Option<&Auth>;
    fn clear_auth(&mut self);
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

    fn set_auth(
        &mut self,
        uid: Uid,
        refresh_token: SecretString,
        access_token: SecretString,
        scope: AuthScope,
    ) {
        self.auth = Some(Auth {
            uid,
            refresh_token,
            access_token,
            scope,
        })
    }

    fn set_scopes(&mut self, scope: AuthScope) -> Option<&Auth> {
        let Some(auth) = &mut self.auth else {
            return None;
        };

        auth.scope = scope;
        Some(auth)
    }

    fn clear_auth(&mut self) {
        self.auth = None;
    }
}
pub type ArcAuthStore = Arc<RwLock<dyn AuthStore>>;

pub fn new_arc_auth_store<T: AuthStore>(auth: T) -> ArcAuthStore {
    Arc::new(RwLock::new(auth))
}
