use std::time::Duration;

use crate::models::ModelExtension;
use itertools::Itertools;
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether};
use tokio::time::timeout;

use crate::datatypes::{InitializedComponentKey, InitializedComponentState};

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

    /// State which defined whether component has been initialized or not.
    #[DbField]
    state: InitializedComponentState,

    /// The internal row ID of the record in the database. This is assigned by
    /// `SQLite`, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl InitializedComponent {
    /// Save or update initialized component.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is updated correctly in the database.
    ///
    /// This method ensures that there is only one initialization status per key in the table.
    /// Otherwise, it overwrites old record.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails
    ///
    async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(existing) = Self::find_by_id(self.key, bond).await? {
            self.row_id = existing.row_id;
        }

        <Self as Model>::save(self, bond).await
    }

    /// Returns a list of states for all dependencies with a single SQL query
    ///
    async fn states_for_deps(
        keys: &[InitializedComponentKey],
        tether: &Tether,
    ) -> Result<Vec<InitializedComponentState>, StashError> {
        let states = Self::find_by_ids(keys.iter().copied(), tether)
            .await?
            .into_iter()
            .map(|c| c.state)
            .collect();
        Ok(states)
    }

    /// Merges states together to produce an information if the component does still depend on some other components
    ///
    /// If the input iterator is empty, it returns `NotInitialized` state.
    ///
    fn coalesce_states(
        states: impl IntoIterator<Item = InitializedComponentState>,
    ) -> InitializedComponentState {
        states
            .into_iter()
            .coalesce(|a, b| match (a, b) {
                // If at least one of them failed, we consider it a failure
                (_, InitializedComponentState::Failed) | (InitializedComponentState::Failed, _) => {
                    Ok(InitializedComponentState::Failed)
                }
                // If at least one of them are not initialized, we consider all not initialized
                (_, InitializedComponentState::NotInitialized)
                | (InitializedComponentState::NotInitialized, _) => {
                    Ok(InitializedComponentState::NotInitialized)
                }

                // Otherwise, it's a success
                _ => Ok(InitializedComponentState::Succeeded),
            })
            .next()
            .unwrap_or_default()
    }

    async fn state(
        key: InitializedComponentKey,
        tether: &Tether,
    ) -> Result<InitializedComponentState, StashError> {
        let state = Self::find_by_id(key, tether)
            .await?
            .map(|c| c.state)
            .unwrap_or_default();
        Ok(state)
    }
    /// Checks whether component has been initialized
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    async fn is_initialized(
        key: InitializedComponentKey,
        tether: &Tether,
    ) -> Result<bool, StashError> {
        let state = Self::state(key, tether).await?;
        Ok(matches!(state, InitializedComponentState::Succeeded))
    }

    /// Mark component as initialized by running initialization async closure s.
    /// This operation is **idempotent**. If the component is already initialized, it becomes no-op.
    ///
    /// # Dependencies
    ///
    /// Dependency is an another component that needs to be initialized before this one.
    /// In case of leaf components, leave `&[]`.
    ///
    /// # Async Closures
    ///
    /// There are two closures:
    ///
    /// * `fetch` that does not require a transaction, and does not wait for the dependencies,
    /// * `store` that provides a transaction, and is executed only if all dependencies are initialized
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    #[tracing::instrument(skip(tether, fetch, store))]
    pub async fn initialize<E, CTX>(
        key: InitializedComponentKey,
        dependencies: &[InitializedComponentKey],
        mut tether: Tether,
        fetch: impl AsyncFnOnce() -> Result<CTX, E> + '_,
        store: impl AsyncFnOnce(&Bond<'_>, CTX) -> Result<(), E> + '_,
    ) -> Result<(), InitializationError<E>>
    where
        E: std::fmt::Debug,
        CTX: Send,
    {
        tracing::debug!("Initializing");
        if Self::is_initialized(key, &tether).await? {
            tracing::debug!("Already initialized");
            // We already initialized it
            return Ok(());
        }

        // We split the initialization into two phases.
        // First we fetch data. We assume, that fetching from BE does not depend on any
        // other component.
        //
        // Then we store the data, which depends on other components.
        tracing::debug!("Fetching");
        let fetched = match fetch().await {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("Failed the initialization in fetched stage: {e:?}");
                Self::fail(key, &mut tether).await?;
                return Err(InitializationError::InitializationFailed(e));
            }
        };

        tracing::debug!("Fetched");

        if let Err(e) = Self::wait_for_dependencies(key, dependencies, &tether).await {
            tracing::error!("Component dependencies error: {e:?}");
            Self::fail(key, &mut tether).await?;
            return Err(e.into());
        }

        tracing::trace!("Storing. Creating a transaction");
        let tx = tether.transaction().await?;

        tracing::trace!("Storing");
        let res = store(&tx, fetched).await;
        tracing::trace!("Stored");

        let state = if res.is_err() {
            InitializedComponentState::Failed
        } else {
            InitializedComponentState::Succeeded
        };

        tracing::debug!("Marking as {state:?}");

        Self {
            key,
            state,
            row_id: None,
        }
        .save(&tx)
        .await?;

        tracing::trace!("Committing transaction");
        tx.commit().await?;
        tracing::trace!("Committed");

        res.map_err(InitializationError::InitializationFailed)
    }

    async fn fail(key: InitializedComponentKey, tether: &mut Tether) -> Result<(), StashError> {
        let tx = tether.transaction().await?;
        Self {
            key,
            state: InitializedComponentState::Failed,
            row_id: None,
        }
        .save(&tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Wait until dependencies are initialized.
    /// If dependency fails to initialize, this component also fails.
    /// That creates a cascade effect.
    ///
    async fn wait_for_dependencies(
        key: InitializedComponentKey,
        dependencies: &[InitializedComponentKey],
        tether: &Tether,
    ) -> Result<(), DependencyInitializationError> {
        tracing::debug!("Waiting for dependencies: {dependencies:?}");
        // Early exit for leafs
        if dependencies.is_empty() {
            tracing::debug!("There are no dependencies");
            return Ok(());
        }

        let handle =
            tether.subscribe_to(|sender| Box::new(InitializedDependenciesWatcher { sender }))?;

        // We already have a handle, but let's also check dependencies at least once, in case something is already initialized.
        if Self::check_dependencies(key, dependencies, tether).await? {
            return Ok(());
        }

        let receiver = &handle.receiver;
        loop {
            // Just in case watcher does not react to table change, we still want to periodically check.
            let fut = timeout(Duration::from_secs(1), async {
                receiver
                    .recv_async()
                    .await
                    .map_err(|_| StashError::WatcherError("Connection lost".to_owned()))
            })
            .await;
            if let Ok(Err(e)) = fut {
                return Err(e.into());
            }
            if Self::check_dependencies(key, dependencies, tether).await? {
                return Ok(());
            }
        }
    }

    /// Check if all dependencies are initialized.
    /// If at least one fails, it returns error.
    /// If all succeeed, it returns true.
    /// Otherwise, false
    async fn check_dependencies(
        key: InitializedComponentKey,
        dependencies: &[InitializedComponentKey],
        tether: &Tether,
    ) -> Result<bool, DependencyInitializationError> {
        let states = Self::states_for_deps(dependencies, tether).await?;
        let state = Self::coalesce_states(states);

        tracing::debug!("Checking state of dependencies: {state:?}");

        match state {
            InitializedComponentState::Failed => {
                Err(DependencyInitializationError::DependencyFailed(key))
            }
            InitializedComponentState::Succeeded => Ok(true),
            InitializedComponentState::NotInitialized => Ok(false),
        }
    }
}

/// Error that happened during the initialization of user context
///
#[derive(Debug, thiserror::Error)]
pub enum InitializationError<E> {
    #[error("Initialization failed: {0:?}")]
    InitializationFailed(E),

    #[error("Initialization of the dependency {0:?} failed")]
    DependencyFailed(InitializedComponentKey),

    #[error(transparent)]
    Stash(#[from] StashError),
}

impl<E> From<DependencyInitializationError> for InitializationError<E> {
    fn from(value: DependencyInitializationError) -> Self {
        match value {
            DependencyInitializationError::DependencyFailed(initialized_component_key) => {
                Self::DependencyFailed(initialized_component_key)
            }
            DependencyInitializationError::Stash(stash_error) => Self::Stash(stash_error),
        }
    }
}

/// Error that happened while waiting for the dependency
#[derive(Debug, thiserror::Error)]
pub enum DependencyInitializationError {
    #[error("Initialization of the dependency for {0:?} failed")]
    DependencyFailed(InitializedComponentKey),

    #[error(transparent)]
    Stash(#[from] StashError),
}

struct InitializedDependenciesWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for InitializedDependenciesWatcher {
    fn tables(&self) -> Vec<String> {
        vec![InitializedComponent::table_name().to_string()]
    }

    fn on_tables_changed(&self, _: &std::collections::BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for InitializedComponent: {e:?}");
            })
            .ok();
    }
}
