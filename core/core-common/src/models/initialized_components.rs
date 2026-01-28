use std::sync::Arc;
use std::time::Instant;

use crate::models::ModelExtension;
use itertools::Itertools;
use sqlite_watcher::watcher::TableObserver;
use stash::UserDb;
use stash::exports::Transaction;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use tracing::{debug, error, info, trace};

use crate::datatypes::{InitializationKey, InitializedComponentState};

/// A table that stores information about which component/service/provider is initialized and ready to work.
/// It prevents us from double-initialization, as well as informs when the application is ready for user interactions or events from the network.
/// If the entry exists, it means it has been initialized
///
#[derive(Debug, Eq, Model, PartialEq, Clone)]
#[TableName("initialized_components")]
#[Database(UserDb)]
pub struct InitializedComponent {
    #[IdField]
    key: String,

    #[DbField]
    state: InitializedComponentState,
}

impl InitializedComponent {
    /// Returns a list of states for all dependencies with a single SQL query
    ///
    async fn states_for_deps(
        keys: &[InitializationKey],
        tether: &Tether,
    ) -> Result<Vec<InitializedComponentState>, StashError> {
        let states = Self::find_by_ids(keys.iter().copied().map(From::from), tether)
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

    /// Returns a state matching given initialization key if exists.
    /// Otherwise returns not initialized status
    ///
    pub async fn state(
        key: InitializationKey,
        tether: &Tether,
    ) -> Result<InitializedComponentState, StashError> {
        let state = Self::find_by_id(key.into(), tether)
            .await?
            .map(|c| c.state)
            .unwrap_or_default();
        Ok(state)
    }

    /// Checks whether component has been initialized
    ///
    async fn is_initialized(key: InitializationKey, tether: &Tether) -> Result<bool, StashError> {
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
    #[tracing::instrument(skip_all, fields(key = key.0, dependencies = ?dependencies))]
    pub async fn initialize<E, CTX>(
        watcher: Arc<InitializationWatcher>,
        key: InitializationKey,
        dependencies: &[InitializationKey],
        mut tether: Tether,
        fetch: impl AsyncFnOnce() -> Result<CTX, E>,
        store: impl FnOnce(&Transaction<'_>, CTX) -> Result<(), E> + 'static + Send,
    ) -> Result<(), InitializationError<E>>
    where
        E: std::fmt::Debug + Send + 'static,
        CTX: Send + 'static,
    {
        if Self::is_initialized(key, &tether).await? {
            tracing::info!("Already initialized");
            // We already initialized it
            return Ok(());
        }

        tracing::info!("Initializing");

        // We split the initialization into two phases.
        // First we fetch data. We assume, that fetching from BE does not depend on any
        // other component.
        //
        // Then we store the data, which depends on other components.
        debug!("Fetching");
        let t0 = Instant::now();
        let fetched = match fetch().await {
            Ok(o) => o,
            Err(e) => {
                tracing::error!(
                    "Failed the initialization in fetched stage after {:?}: {e:?}",
                    t0.elapsed()
                );
                Self::fail(key, &mut tether).await?;
                return Err(InitializationError::InitializationFailed(e));
            }
        };
        let t0 = t0.elapsed();

        debug!("Fetched");
        if let Err(e) = Self::wait_for_dependencies(dependencies, &watcher, &tether).await {
            tracing::error!("Component dependencies error: {e:?}");
            Self::fail(key, &mut tether).await?;
            return Err(e.into());
        }

        trace!("Storing. Creating a transaction");
        let t1 = Instant::now();
        let res: Result<(), InitializationError<E>> = tether
            .sync_tx_returning(move |tx| {
                trace!("Storing");

                match store(tx, fetched) {
                    Ok(()) => {
                        Self {
                            key: key.into(),
                            state: InitializedComponentState::Succeeded,
                        }
                        .save_sync(tx)?;
                        Ok(Ok(()))
                    }
                    Err(e) => {
                        Self {
                            key: key.into(),
                            state: InitializedComponentState::Failed,
                        }
                        .save_sync(tx)?;
                        Ok(Err(InitializationError::InitializationFailed(e)))
                    }
                }
            })
            .await?;
        info!("Fetch took {t0:?}, store took {:?}", t1.elapsed());
        res
    }

    /// Sets state immediately
    ///
    /// # Warning
    ///
    /// This does not check whether the key was already initialized or not
    ///
    pub async fn set_state(
        key: InitializationKey,
        state: InitializedComponentState,
        tether: &mut Tether,
    ) -> Result<(), StashError> {
        tether
            .tx(async |tx| Self::set_state_tx(key, state, tx).await)
            .await?;

        Ok(())
    }

    pub async fn set_state_tx(
        key: InitializationKey,
        state: InitializedComponentState,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        Self {
            key: key.into(),
            state,
        }
        .save(bond)
        .await
    }

    async fn fail(key: InitializationKey, tether: &mut Tether) -> Result<(), StashError> {
        Self::set_state(key, InitializedComponentState::Failed, tether).await
    }

    /// Wait until dependencies are initialized.
    /// If dependency fails to initialize, this component also fails.
    /// That creates a cascade effect.
    ///
    pub async fn wait_for_dependencies(
        dependencies: &[InitializationKey],
        watcher: &InitializationWatcher,
        tether: &Tether,
    ) -> Result<(), DependencyInitializationError> {
        debug!("Waiting for dependencies: {dependencies:?}");
        // Early exit for leafs
        if dependencies.is_empty() {
            debug!("There are no dependencies");
            return Ok(());
        }

        let mut handle = watcher.subscribe();

        // We already have a handle, but let's also check dependencies at least once, in case something is already initialized.
        if Self::check_dependencies(dependencies, tether).await? {
            return Ok(());
        }

        loop {
            if let Err(tokio::sync::broadcast::error::RecvError::Closed) = handle.recv().await {
                return Err(
                    StashError::WatcherError("Watcher closed prematurely".to_owned()).into(),
                );
            }
            if Self::check_dependencies(dependencies, tether).await? {
                return Ok(());
            }
        }
    }

    /// Check if all dependencies are initialized.
    /// If at least one fails, it returns error.
    /// If all succeeed, it returns true.
    /// Otherwise, false
    async fn check_dependencies(
        dependencies: &[InitializationKey],
        tether: &Tether,
    ) -> Result<bool, DependencyInitializationError> {
        let states = Self::states_for_deps(dependencies, tether).await?;
        let state = Self::coalesce_states(states);

        trace!("Checking state of dependencies: {state:?}");

        match state {
            InitializedComponentState::Succeeded => Ok(true),
            // If dependency failed it means either:
            // * It just failed, and this task will be aborted very soon anyway.
            // * Or, it failed in the previous run, it is currently running and it will change from Failed -> Succeeded.
            //   Then we need to wait patiently
            InitializedComponentState::Failed | InitializedComponentState::NotInitialized => {
                Ok(false)
            }
        }
    }
}

/// Error that happened during the initialization of user context
///
#[derive(Debug, thiserror::Error)]
pub enum InitializationError<E> {
    #[error("Initialization failed: {0:?}")]
    InitializationFailed(E),

    #[error(transparent)]
    Stash(#[from] StashError),
}

impl<E> From<DependencyInitializationError> for InitializationError<E> {
    fn from(value: DependencyInitializationError) -> Self {
        match value {
            DependencyInitializationError::Stash(stash_error) => Self::Stash(stash_error),
        }
    }
}

/// Error that happened while waiting for the dependency
#[derive(Debug, thiserror::Error)]
pub enum DependencyInitializationError {
    #[error(transparent)]
    Stash(#[from] StashError),
}

/// Watches for the changes in table and notifies multiple threads.
pub struct InitializationWatcher {
    /// Receives information about changes in the database
    handle: WatcherHandle,

    /// Broadcasts received notification further to components
    sender: tokio::sync::broadcast::Sender<()>,
}

/// Handle that allows component to wait for dependency
pub struct InitializationWatcherHandle(tokio::sync::broadcast::Receiver<()>);

impl std::ops::Deref for InitializationWatcherHandle {
    type Target = tokio::sync::broadcast::Receiver<()>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for InitializationWatcherHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl InitializationWatcher {
    pub async fn new(stash: &Stash<UserDb>) -> Result<Arc<Self>, StashError> {
        let handle = stash
            .subscribe_to(|sender| Box::new(InitializedDependenciesTableWatcher { sender }))
            .await?;
        let (tx, _rx) = tokio::sync::broadcast::channel(1);

        let this = Self { handle, sender: tx };

        Ok(Arc::new(this))
    }

    /// Subscribe to changes in that table
    #[must_use]
    pub fn subscribe(&self) -> InitializationWatcherHandle {
        InitializationWatcherHandle(self.sender.subscribe())
    }

    /// Tokio task
    pub async fn task(self: Arc<Self>) -> Result<(), StashError> {
        let receiver = &self.handle.receiver;

        loop {
            receiver
                .recv_async()
                .await
                .inspect_err(|e| {
                    tracing::error!("Initialization watcher failed to observe table: {e:?}");
                })
                .map_err(|_| StashError::WatcherError("Connection lost".to_owned()))?;

            // We ignore errors if no one is listening.
            // There are two cases:
            // * Either all tasks finished or
            // * None of the tasks subscribed yet.
            //
            // In first case we are going to abort this task anyway,
            // In the second - we want to keep this task spinning - someone might start listening soon
            _ = self.sender.send(()).inspect_err(|e| {
                tracing::warn!("Initialization watcher failed to notify about change: {e:?}");
            });
        }
    }
}

struct InitializedDependenciesTableWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for InitializedDependenciesTableWatcher {
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
