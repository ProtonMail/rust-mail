use proton_core_common::models::ModelExtension;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether};

use crate::datatypes::InitializedComponentKey;

/// A table that stores information about which component/service/provider is initialized and ready to work.
/// It prevents us from double-initialization, as well as informs when the application is ready for user interactions or events from the network.
/// If the entry exists, it means it has been initialized
///
#[derive(Debug, Eq, Model, PartialEq, Clone, Copy)]
#[TableName("initialized_components")]
pub struct InitializedComponent {
    /// Key which defines which component has been initialized
    #[IdField]
    key: InitializedComponentKey,

    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl InitializedComponent {
    /// Checks whether component has been initialized
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    pub async fn initialized(
        key: InitializedComponentKey,
        tether: &Tether,
    ) -> Result<bool, StashError> {
        Ok(Self::find_by_id(key, tether).await?.is_some())
    }

    /// Mark component as initialized by running initialization async closure.
    /// This operation is **idempotent**. If the component is already initialized, it becomes no-op.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    pub async fn initialize<E>(
        key: InitializedComponentKey,
        tx: &Bond<'_>,
        f: impl AsyncFnOnce(&Bond<'_>) -> Result<(), E> + '_,
    ) -> Result<(), E>
    where
        E: From<StashError>,
    {
        if Self::initialized(key, tx).await? {
            // We already initialized it
            return Ok(());
        }

        f(tx).await?;

        Self { key, row_id: None }.save(tx).await?;

        Ok(())
    }
}
