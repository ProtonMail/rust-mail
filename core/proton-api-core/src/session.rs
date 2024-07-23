#![allow(clippy::module_name_repetitions)]

use crate::auth::{Auth, UserKeySecret};
use crate::service::ApiServiceError;
use crate::services::proton::{Config as ApiConfig, Proton};
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;

pub trait CoreSession {
    #[must_use]
    fn api(&self) -> &Proton;
}

/// Authenticated Session from which one can access data/functionality restricted to authenticated
/// users.
#[derive(Clone)]
pub struct Session {
    pub(crate) auth: Arc<AsyncRwLock<Option<Auth>>>,
    api: Proton,
}

impl Session {
    #[must_use]
    pub fn new(api_config: ApiConfig) -> Self {
        let auth = Arc::new(AsyncRwLock::new(None));
        Self {
            api: Proton::new(api_config, None, Arc::clone(&auth)),
            auth,
        }
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

    /// Exposes the user key secret from the auth store to unlock user keys.
    ///
    /// Returns [`None`] if the auth store is not available or no key secret is
    /// stored.
    ///
    pub async fn expose_key_secret(&self) -> Option<UserKeySecret> {
        self.auth
            .read()
            .await
            .as_ref()
            .and_then(|auth| auth.key_secret.clone())
    }

    /// Logout the user and invalidate the current session.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn logout(&self) -> Result<(), ApiServiceError> {
        self.api.delete_auth().await?;
        *self.auth.write().await = None;
        Ok(())
    }
}

impl CoreSession for Session {
    fn api(&self) -> &Proton {
        &self.api
    }
}
