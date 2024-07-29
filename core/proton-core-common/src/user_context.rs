pub use self::keys::*;
use crate::cache::ProtonCache;
use crate::datatypes::RemoteId;
use crate::user_context::images_logo::Key;
use crate::CoreContextResult;
use proton_api_core::session::Session;
use proton_sqlite3::MigratorError;
use stash::stash::Stash;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;

pub mod images_logo;
mod keys;

/// Extra initializer for the user database.
pub trait UserDatabaseInitializer: Send + Sync {
    /// Initialize the database as needed by running database migrations.
    ///
    /// # Errors
    /// Return error if the migration failed.
    fn initialize(&self, stash: &Stash) -> Result<(), MigratorError>;
}

/// Contains all the relevant information to an initialize user session.
#[derive(Clone)]
pub struct UserContext {
    session: Session,
    stash: Stash,
    user_id: RemoteId,
    pub(self) key_manager: Arc<CryptoKeyManager>,
    pub images_logo_cache: Arc<ProtonCache<Key>>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl UserContext {
    pub(crate) fn new(
        session: Session,
        stash: Stash,
        id: RemoteId,
        cache_path: PathBuf,
        cache_size: u32,
    ) -> CoreContextResult<Self> {
        Ok(Self {
            session,
            stash,
            user_id: id,
            key_manager: Arc::new(CryptoKeyManager::new()),
            images_logo_cache: Arc::new(ProtonCache::new(cache_path, cache_size)?),
        })
    }

    /// Get the network session.
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Get the network session converted to a type that accepts this type.
    #[must_use]
    pub fn session_as<T: From<Session>>(&self) -> T {
        T::from(self.session.clone())
    }

    /// Get the database connection.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        &self.stash
    }

    /// Get the user id of this context.
    #[must_use]
    pub fn user_id(&self) -> &RemoteId {
        &self.user_id
    }
}
