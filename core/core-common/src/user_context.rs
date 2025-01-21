pub use self::keys::*;
use crate::cache::ProtonCache;
use crate::datatypes::AccountDetails;
use crate::db::account::CoreAccount;
use crate::db::migrations::{migrate_account_db, migrate_core_db};
use crate::models::sender_image_cache::SenderImage;
use crate::{CoreContextError, CoreContextResult};
use proton_api_core::services::proton::common::{AuthId, UserId};
use proton_api_core::session::Session;
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::Stash;
use std::fmt::{Debug, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    session: Session,
    account_stash: Stash,
    user_stash: Stash,
    user_id: UserId,
    session_id: AuthId,
    pub(self) key_manager: Arc<CryptoKeyManager>,
    pub images_logo_cache: Arc<ProtonCache<SenderImage>>,
}

impl Debug for UserContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, " UserContext({})", self.user_id)
    }
}

impl UserContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn new(
        session: Session,
        account_stash: Stash,
        user_stash_path: &Path,
        db_initializers: &[Box<dyn UserDatabaseInitializer>],
        user_id: UserId,
        session_id: AuthId,
        cache_path: PathBuf,
        sender_image_cache_size: u64,
    ) -> CoreContextResult<Arc<Self>> {
        let user_stash = Self::new_user_db(user_stash_path, db_initializers).await?;

        let images_logo_cache = Self::init_sender_image_cache(
            cache_path.join("sender_images"),
            sender_image_cache_size,
            &user_stash,
        )
        .await?;

        Ok(Arc::new(Self {
            session,
            account_stash,
            user_stash,
            user_id,
            session_id,
            key_manager: Arc::new(CryptoKeyManager::new()),
            images_logo_cache,
        }))
    }

    async fn init_sender_image_cache(
        cache_path: PathBuf,
        cache_size: u64,
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
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    /// Retrieves the current user's account details.
    ///
    /// # Returns
    /// - `Err(CoreContextError)` if the account is missing or a database error occurs.
    ///
    /// # Errors
    ///
    /// Returns `CoreContextError` if the account does not exist or if an error occurs
    /// during the database query.
    pub async fn account_details(&self) -> CoreContextResult<AccountDetails> {
        let tether = self.account_stash.connection();
        let user_id = self.user_id();
        let account = CoreAccount::load(user_id.clone(), &tether)
            .await?
            .ok_or_else(|| CoreContextError::AccountMissing(user_id.clone()))?;

        Ok(account.details())
    }

    /// Get the session id of this context.
    #[must_use]
    pub fn session_id(&self) -> &AuthId {
        &self.session_id
    }

    async fn new_user_db(
        path: &Path,
        db_initializers: &[Box<dyn UserDatabaseInitializer>],
    ) -> Result<Stash, MigratorError> {
        let stash = Stash::new(Some(path))?;
        debug!("initializing core database");
        // initialize core db
        migrate_account_db(&stash).await?;
        migrate_core_db(&stash).await?;
        debug!("initializing user ");
        // initialize user db
        for initializer in db_initializers {
            initializer.initialize(&stash)?;
        }

        Ok(stash)
    }
}
