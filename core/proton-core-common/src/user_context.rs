pub use self::keys::*;
use crate::cache::ProtonCache;
use crate::datatypes::RemoteId;
use crate::models::sender_image_cache::SenderImage;
use crate::{Context, CoreContextResult};
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

    /// A helper to return a boxed trait object.
    fn boxed(self) -> Box<dyn UserDatabaseInitializer>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

/// Contains all the relevant information to an initialize user session.
#[derive(Clone)]
pub struct UserContext {
    context: Arc<Context>,
    session: Session,
    user_stash: Stash,
    user_id: RemoteId,
    session_id: RemoteId,
    pub(self) key_manager: Arc<CryptoKeyManager>,
    pub images_logo_cache: Arc<ProtonCache<SenderImage>>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl UserContext {
    pub(crate) async fn new(
        context: Arc<Context>,
        session: Session,
        user_stash: Stash,
        user_id: RemoteId,
        session_id: RemoteId,
        cache_path: PathBuf,
        cache_size: u32,
    ) -> CoreContextResult<Self> {
        let images_logo_cache =
            Self::init_sender_image_cache(cache_path, cache_size, &user_stash).await?;

        Ok(Self {
            context,
            session,
            user_stash,
            user_id,
            session_id,
            key_manager: Arc::new(CryptoKeyManager::new()),
            images_logo_cache,
        })
    }

    async fn init_sender_image_cache(
        cache_path: PathBuf,
        cache_size: u32,
        user_stash: &Stash,
    ) -> CoreContextResult<Arc<ProtonCache<SenderImage>>> {
        let cache = ProtonCache::new(
            cache_path.join("images_logo_cache"),
            cache_size,
            user_stash.to_owned(),
        )
        .await?;

        Ok(Arc::new(cache))
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

    /// Get the session id of this context.
    #[must_use]
    pub fn session_id(&self) -> &RemoteId {
        &self.session_id
    }

    /// Set this user as the primary user.
    ///
    /// # Errors
    ///
    /// Returns an error if the database operation fails.
    pub async fn set_primary(&self) -> CoreContextResult<()> {
        self.context.set_primary_account(self.user_id.clone()).await
    }
}
