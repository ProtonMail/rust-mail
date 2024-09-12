#![allow(clippy::module_name_repetitions)]

use crate::auth::{CachedStore, Store, StoreError, UserKeySecret};
use crate::service::ApiServiceError;
use crate::services::proton::{Config as ApiConfig, Proton};
use tokio::sync::RwLock as AsyncRwLock;

/// Core session trait which provides access to the API.
///
/// TODO: Rename this to simply `Api`?
pub trait CoreSession {
    #[must_use]
    fn api(&self) -> &Proton;
}

/// Authenticated API session from which one can access data/functionality restricted to authenticated
/// users.
#[derive(Clone)]
pub struct Session {
    api: Proton,
}

impl Session {
    /// Create a new Session
    ///
    /// # Errors
    ///
    /// Returns error if the API service failed to initialize.
    pub async fn new(
        api_config: ApiConfig,
        store: Option<Box<dyn Store>>,
    ) -> Result<Self, StoreError> {
        Ok(Self {
            api: Proton::new(api_config, None, store).await?,
        })
    }

    /// Fork the current session.
    ///
    /// This call has to be made from a parent session, and forks the current
    /// logged-in user session in order to provide a new session for the same
    /// user.
    ///
    /// If successful, this will return the "Selector" string for the new
    /// session.
    ///
    /// # Errors
    ///
    /// Any of the [`ApiServiceError`] variants could be returned if there is a
    /// problem with the HTTP request.
    ///
    pub async fn fork(&self) -> Result<String, ApiServiceError> {
        self.api
            .post_auth_sessions_forks(Some(self.api.config().app_version.clone()))
            .await
            .map(|r| r.selector)
    }

    /// Fork the current session with a user and a version.
    /// for more details see [`Fork`]
    ///
    /// # Errors
    ///
    /// Any of the [`ApiServiceError`] variants could be returned if there is a
    /// problem with the HTTP request.
    ///
    pub async fn fork_with_version(&self, version: String) -> Result<String, ApiServiceError> {
        self.api
            .post_auth_sessions_forks(Some(version))
            .await
            .map(|r| r.selector)
    }

    /// Exposes the user key secret from the auth store to unlock user keys.
    ///
    /// Returns [`None`] if the auth store is not available or no key secret is
    /// stored.
    ///
    pub async fn expose_key_secret(&self) -> Option<UserKeySecret> {
        self.auth_store()
            .read()
            .await
            .get_secrets()
            .map(|secrets| secrets.key_secret.clone())
    }

    /// Logout the user and invalidate the current session.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn logout(&self) -> Result<(), ApiServiceError> {
        self.api.delete_auth().await?;
        self.auth_store().write().await.clear().await?;
        Ok(())
    }

    pub(crate) fn auth_store(&self) -> &AsyncRwLock<CachedStore> {
        self.api.auth_store()
    }
}

impl CoreSession for Session {
    fn api(&self) -> &Proton {
        &self.api
    }
}
