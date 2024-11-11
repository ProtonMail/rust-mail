use async_trait::async_trait;
use std::ops::DerefMut;
use std::sync::Arc;
use std::{error::Error, ops::Deref};
use tokio::sync::RwLock;

use crate::auth::{Auth, UserKeySecret};

/// A thread-safe, shared store.
pub type DynStore = Arc<RwLock<Box<dyn Store>>>;

/// The error type returned by the store.
pub type StoreError = Box<dyn Error + Send + Sync>;

/// Authentication storage abstraction trait in order to store or load auth data.
#[async_trait]
pub trait Store: Send + Sync + 'static {
    /// Set the name or address used to authenticate.
    fn get_name_or_addr(&self) -> Option<&String>;

    /// Set the name or address used to authenticate.
    fn set_name_or_addr(&mut self, name_or_addr: &str);

    /// Get the current auth session data.
    async fn get_auth(&self) -> Auth;

    /// Set the auth session data.
    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError>;

    /// Get the user's key secret.
    async fn get_key_secret(&self) -> Option<UserKeySecret>;

    /// Set the user's key secret.
    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError>;

    /// Clear all stored data.
    async fn clear(&mut self) -> Result<(), StoreError>;
}

#[async_trait]
impl<S: ?Sized + Store> Store for Box<S> {
    /// Set the name or address used to authenticate.
    fn get_name_or_addr(&self) -> Option<&String> {
        self.deref().get_name_or_addr()
    }

    /// Set the name or address used to authenticate.
    fn set_name_or_addr(&mut self, name_or_addr: &str) {
        self.deref_mut().set_name_or_addr(name_or_addr)
    }

    async fn get_auth(&self) -> Auth {
        self.deref().get_auth().await
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError> {
        self.deref_mut().set_auth(auth).await
    }

    async fn get_key_secret(&self) -> Option<UserKeySecret> {
        self.deref().get_key_secret().await
    }

    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError> {
        self.deref_mut().set_key_secret(secret).await
    }

    async fn clear(&mut self) -> Result<(), StoreError> {
        self.deref_mut().clear().await
    }
}

/// A dummy store implementation, used when no store is provided.
#[derive(Debug, Default)]
pub(crate) struct TempStore {
    auth: Auth,
    secret: Option<UserKeySecret>,
    name_or_addr: Option<String>,
}

impl TempStore {
    pub fn boxed() -> Box<dyn Store> {
        Box::new(Self::default())
    }
}

#[async_trait]
impl Store for TempStore {
    fn get_name_or_addr(&self) -> Option<&String> {
        self.name_or_addr.as_ref()
    }

    fn set_name_or_addr(&mut self, name_or_addr: &str) {
        self.name_or_addr = Some(name_or_addr.to_owned());
    }

    async fn get_auth(&self) -> Auth {
        self.auth.clone()
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError> {
        self.auth = auth;

        Ok(())
    }

    async fn get_key_secret(&self) -> Option<UserKeySecret> {
        self.secret.clone()
    }

    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError> {
        self.secret = Some(secret);

        Ok(())
    }

    async fn clear(&mut self) -> Result<(), StoreError> {
        self.auth = Auth::None;
        self.secret = None;

        Ok(())
    }
}
