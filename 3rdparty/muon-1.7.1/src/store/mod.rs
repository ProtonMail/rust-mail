//! ## Store
//!
//! This module defines the types required to implement a persistent session
//! store that may or may not be used in a Muon client. A persistent store
//! allows to store in a persistent memory (e.g., a database, a file) the user
//! [`muon::client::Auth`](`crate::client::Auth`) session information.
//! It is useful for someone that wants to provide SSO capability to his
//! application. However, it *MUST NOT* be used as an IPC mechanism nor a
//! Session synchronization point.
//!
//! [`Store`](`crate::client::Auth`) implementers must handle internal errors
//! themselves see `examples/fallible-store` for an example. The motivation for
//! this was to reduce the error handling code duplication and instaead to have
//! a centralized place to handle such errors.
//!
//! In state inconsistency situations, where the persistent storage has data
//! that is not up-to-date, the recommendation is to simply use that information
//! and let the Proton API telling what to do. For instance:
//! - if the state in the persistent storage is logged, but we aren't, the API
//!   will tell to log-in again.
//! - if the state in the persistent storage is logged out, but we aren't, we
//!   will try to log-in, and the API will tell that we already are.
//!
//! see [here](https://protonmail.slack.com/archives/C06FUDRJ9MJ/p1722317918648489) for the complete discussion.
//!
//! ### Example without error handling
//!
//! ```
//! use muon::store::{Store, StoreError};
//! use muon::env::EnvId;
//! use muon::client::Auth;
//! /// A dummy in memory persistent storage
//! #[derive(Debug)]
//! pub struct MyAtlasPersistentStorage(EnvId, Auth);
//! impl MyAtlasPersistentStorage {
//!     // create the env and set Auth::None
//!     pub fn new() -> Self {
//!         Self(EnvId::new_atlas(), Default::default())
//!     }
//! }
//! #[async_trait::async_trait]
//! impl Store for MyAtlasPersistentStorage {
//!     // retrieve the env
//!     fn env(&self) -> EnvId {
//!         self.0.clone()
//!     }
//!     // retrieve the auth
//!     async fn get_auth(&self) -> Auth {
//!         self.1.clone()
//!     }
//!     // set the auth
//!     async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError> {
//!         self.1 = auth;
//!         // retrieve the auth that is currently stored
//!         Ok(self.get_auth().await)
//!     }
//! }
//! ```
//!
//! ### Example with error handling
//!
//! ```
//! use muon::store::{Store, StoreError};
//! use muon::env::EnvId;
//! use muon::client::Auth;
//! # impl From<std::io::Error> for FallibleFileStoreErrors {
//! #     fn from(value: std::io::Error) -> Self {
//! #         match value.kind() {
//! #             std::io::ErrorKind::NotFound => Self::NotFound,
//! #             std::io::ErrorKind::WriteZero => Self::Full,
//! #             kind => Self::Other(kind),
//! #         }
//! #     }
//! # }
//! #
//! # #[derive(Debug, Clone, Copy)]
//! # struct PersistentStorageFile;
//! # fn create_file() -> std::io::Result<PersistentStorageFile> {Ok(PersistentStorageFile)}
//! # fn read_auth(of: &PersistentStorageFile) -> std::io::Result<Auth> {Ok(Default::default())}
//! # fn write_file(of: &PersistentStorageFile, auth: &Auth) -> std::io::Result<()> {Ok(())}
//! # fn ask_to_free_disk_space() {}
//! # fn ask_to_clear_cache() {}
//! # fn recreate_storage_file() {}
//! # fn display_error_modal(err: std::io::ErrorKind) {}
//! # use std::path::Path;
//! # fn get_auth_file_path(of : &PersistentStorageFile) -> impl AsRef<Path> { Path::new("...") }
//! /// The error returned by my store
//! enum FallibleFileStoreErrors {
//!     /// The disk is full
//!     Full,
//!     /// The file is not found
//!     NotFound,
//!     /// The file is corrupted/not interpretable
//!     Corrupted,
//!     /// Anything else that comes from IO
//!     Other(std::io::ErrorKind),
//! }
//!
//! /// A functor that handle an error
//! #[derive(Debug, Default, Clone)]
//! struct StoreErrorHandler;
//!
//! impl StoreErrorHandler {
//!     /// Deal with my concrete error type.
//!     /// Note: it is important that we can match the type to ensure everything is
//!     /// treated correctly.
//!     pub fn handle_error(&self, err: FallibleFileStoreErrors) {
//!         // handle all errors and treat them differently
//!         match err {
//!             FallibleFileStoreErrors::Full => ask_to_free_disk_space(),
//!             FallibleFileStoreErrors::Corrupted => ask_to_clear_cache(),
//!             FallibleFileStoreErrors::NotFound => recreate_storage_file(),
//!             FallibleFileStoreErrors::Other(e) => display_error_modal(e),
//!         }
//!     }
//! }
//!
//! /// A store that persist in a file and that can fail
//! #[derive(Debug, Clone)]
//! struct FallibleFileStore {
//!     env: EnvId, // the target env for this storage
//!     of: PersistentStorageFile,  // where my auth will be stored
//!     err_handler: StoreErrorHandler, // the handler for the errors
//! }
//!
//! impl FallibleFileStore {
//!     /// Create a prod file storage, it mostly create the initial file with
//!     /// Auth::none in it
//!     pub fn prod(err_handler: StoreErrorHandler) -> std::io::Result<Self> {
//!         // create the file and if you can't return the IO error
//!         create_file().and_then(|of| {
//!             let mut store = Self {
//!                 env: EnvId::new_prod(),
//!                 of,
//!                 err_handler,
//!             };
//!             let _ = store.set_auth(Auth::None);
//!             Ok(store)
//!         })
//!     }
//!     /// get the file path
//!     pub fn auth_file_path(&self) -> impl AsRef<Path> {
//!         get_auth_file_path(&self.of)
//!     }
//! }
//!
//! #[async_trait::async_trait]
//! impl Store for FallibleFileStore {
//!     fn env(&self) -> EnvId {
//!         self.env.clone()
//!     }
//!
//!     async fn get_auth(&self) -> Auth {
//!         // Try to read the file, if an error occurs ask the handler to deal with it
//!         read_auth(&self.of).unwrap_or_else(|e| {
//!             self.err_handler.handle_error(e.into());
//!             Default::default()
//!         })
//!     }
//!
//!     async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError> {
//!         // try to write the auth to the file, if it does not work, handle the error
//!         if let Err(e) = write_file(&self.of, &auth) {
//!             self.err_handler.handle_error(e.into());
//!             return Err(StoreError);
//!         }
//!         Ok(auth)
//!     }
//! }
//! ```

use crate::auth::Auth;
use crate::common::IntoDyn;
use crate::env::EnvId;
#[allow(unused_imports)] // Seems to be used below
use crate::export;
use async_trait::async_trait;
use muon_proc::{autoimpl, derive_dyn};
use thiserror::Error;

export! {
    /// Implements a thread-safe wrapper around [`Store`].
    mod safe (as pub(crate));

    /// Implements an in-memory store.
    mod inmemory (as pub(crate));
}

/// An error indicating that we couldn't store an [`Auth`] in a [`Store`]
#[derive(Debug, Error)]
#[error("failed to store auth")]
pub struct StoreError;

/// An interface to persistent storage.
#[async_trait]
#[autoimpl(for(DynStore))]
#[derive_dyn(Debug)]
pub trait Store: Send + Sync + 'static {
    /// Get the environment to which this store is bound.
    fn env(&self) -> EnvId;

    /// Get the current auth.
    ///
    /// # Errors
    ///
    /// Returns an error if the auth cannot be retrieved.
    async fn get_auth(&self) -> Auth;

    /// Set the current auth.
    ///
    /// # Errors
    ///
    /// Returns an error if the auth cannot be stored.
    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError>;
}

/// A dynamic store.
pub type DynStore = Box<dyn Store>;

impl<This: Store> IntoDyn<DynStore> for This {
    fn into_dyn(self) -> DynStore {
        Box::new(self)
    }
}
