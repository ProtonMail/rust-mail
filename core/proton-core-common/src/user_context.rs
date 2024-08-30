pub use self::keys::*;
use crate::cache::ProtonCache;
use crate::datatypes::RemoteId;
use crate::db::session::UserSessionState;
use crate::user_context::images_logo::Key;
use crate::CoreContextResult;
use proton_api_core::session::Session;
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::Stash;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::debug;

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
    user_stash: Stash,
    session_stash: Stash,
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
        user_stash: Stash,
        session_stash: Stash,
        user_id: RemoteId,
        mut cache_path: PathBuf,
        cache_size: u32,
    ) -> CoreContextResult<Self> {
        cache_path.push("images_logo_cache");
        Ok(Self {
            session,
            user_stash,
            session_stash,
            user_id,
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
        &self.user_stash
    }

    /// Get the user id of this context.
    #[must_use]
    pub fn user_id(&self) -> &RemoteId {
        &self.user_id
    }

    /// Get the state of the session.
    ///
    /// If the session has no state (i.e. it was never marked as active), this will return `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn state(&self) -> CoreContextResult<Option<UserSessionState>> {
        Ok(UserSessionState::find_by_user_id(self.user_id.clone(), &self.session_stash).await?)
    }

    /// Mark this session as active.
    ///
    /// This updates the last active timestamp of this session in the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn set_active(&self) -> CoreContextResult<()> {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();

        let mut state = UserSessionState {
            user_id: self.user_id.clone(),
            last_active_ts: now,
            row_id: None,
            stash: None,
        };

        if let Some(existing) = self.state().await? {
            debug!("updating existing session state");
            state.row_id = existing.row_id;
        } else {
            debug!("creating new session state");
            debug_assert!(state.row_id.is_none());
        }

        state.save_using(&self.session_stash).await?;

        Ok(())
    }

    /// Return whether the session is active.
    ///
    /// A session is considered active if it is the most recent session for the user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn is_active(&self) -> CoreContextResult<bool> {
        if let Some(last) = UserSessionState::find_last(&self.session_stash).await? {
            Ok(last.user_id == self.user_id)
        } else {
            Ok(false)
        }
    }
}
