use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::default_location::IncomingDefaultLocation;
use crate::models::{LabelWithCounters, MailSettings, StoreLabelCounters};
use crate::{MailContextError, MailUserContext};
use futures::try_join;
use proton_core_common::datatypes::InitializationKey;
use proton_core_common::models::{
    Address, Contact, InitializationError, InitializationWatcher, InitializedComponent, User,
};
use proton_event_loop::EventLoopError;
use proton_task_service::AsyncTaskResult;
use tokio::task::JoinHandle;
use tracing::{Level, debug, error, warn};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum MailUserContextLoadingStage {
    UserSettings,
    MailSettings,
    Addresses,
    Events,
    Labels,
    Counters,
    Contacts,
    IncomingDefaults,
}

impl MailUserContext {
    /// Initialize the mail user context, running all the necessary syncs to ensure the context is ready to be used.
    /// Syncs are mostly run in the parallel, but updating message & conversation count are dependent on labels, so it is run in sequence.
    ///
    /// # Warning
    ///
    /// This function probably should not be called explicitly.
    /// It is called automatically during user context session creation
    ///
    /// # Arguments
    ///
    /// * `ctx` - The mail user context to initialize, it is vital to have it as Arc, as it will be cloned multiple times, and passed to the tokio::task.
    ///
    /// # Returns
    ///
    /// An error if the initialization failed for any reason
    ///
    #[tracing::instrument(level = Level::DEBUG, skip(ctx))]
    pub async fn initialize_async(ctx: Arc<Self>) -> Result<(), MailContextError> {
        let watcher = InitializationWatcher::new(ctx.user_stash())?;
        let watcher_clone = watcher.clone();
        let watcher_task_handle = ctx.spawn(async move { watcher_clone.task().await });

        let t0 = Instant::now();
        let labels = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            LabelWithCounters::initialize(watcher, ctx.api(), ctx.user_stash()).await
        });
        let counters = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            StoreLabelCounters::initialize(watcher, ctx.api(), ctx.user_stash()).await
        });
        let contacts = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            Contact::initialize(watcher, ctx.api(), ctx.user_stash()).await
        });

        let event_loop = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            initialize_event_loop(watcher, ctx.as_ref()).await
        });
        let user_settings = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            User::initialize_with_settings(watcher, ctx.api(), ctx.user_stash()).await
        });
        let mail_settings = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            MailSettings::initialize(watcher, ctx.api(), ctx.user_stash()).await
        });
        let addresses = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            Address::initialize(watcher, ctx.api(), ctx.user_stash()).await
        });
        let inc_defs = ctx.spawn_init(&watcher, |ctx, watcher| async move {
            IncomingDefaultLocation::initialize(watcher, ctx.api(), ctx.user_stash()).await
        });

        let abort_handles = [
            watcher_task_handle.abort_handle(),
            labels.abort_handle(),
            contacts.abort_handle(),
            counters.abort_handle(),
            event_loop.abort_handle(),
            user_settings.abort_handle(),
            mail_settings.abort_handle(),
            addresses.abort_handle(),
            inc_defs.abort_handle(),
        ]
        .into_iter()
        .collect::<Vec<_>>();

        let res = try_join!(
            Self::initial_sync_for(MailUserContextLoadingStage::Labels, labels),
            Self::initial_sync_for(MailUserContextLoadingStage::Contacts, contacts),
            Self::initial_sync_for(MailUserContextLoadingStage::Counters, counters),
            Self::initial_sync_for(MailUserContextLoadingStage::Events, event_loop),
            Self::initial_sync_for(MailUserContextLoadingStage::UserSettings, user_settings),
            Self::initial_sync_for(MailUserContextLoadingStage::MailSettings, mail_settings),
            Self::initial_sync_for(MailUserContextLoadingStage::Addresses, addresses),
            Self::initial_sync_for(MailUserContextLoadingStage::IncomingDefaults, inc_defs),
        );

        abort_handles.into_iter().for_each(|a| a.abort());

        match res {
            Ok(_) => {
                debug!("Syncing Complete in {:?}", t0.elapsed());
                Ok(())
            }
            Err(e) => {
                error!("Syncing Failed in {:?}", t0.elapsed());
                Err(e)
            }
        }
    }

    /// Initialize a component.
    #[tracing::instrument(level = Level::DEBUG, skip(handle))]
    async fn initial_sync_for<E>(
        stage: MailUserContextLoadingStage,
        handle: JoinHandle<AsyncTaskResult<Result<(), InitializationError<E>>>>,
    ) -> Result<(), MailContextError>
    where
        E: std::fmt::Debug + Send + Sync + 'static,
        MailContextError: From<E>,
    {
        let t = Instant::now();
        debug!("Begin syncing for {stage:?}");

        let result = handle.await;
        let elapsed = t.elapsed();
        if elapsed > Duration::from_secs(1) {
            warn!("Slow sync for {stage:?}: {elapsed:?}");
        } else {
            debug!("Syncing {stage:?} took {elapsed:?}");
        }

        match result {
            Ok(AsyncTaskResult::Completed(Ok(()))) => Ok(()),
            Ok(AsyncTaskResult::Completed(Err(e))) => match e {
                InitializationError::DependencyFailed(ref dep) => {
                    error!("Failed to sync {e:?} - Dependency failed - {dep}");
                    Ok(())
                }
                InitializationError::InitializationFailed(e) => {
                    let e = e.into();
                    error!("Failed to sync {e:?}");
                    Err(e)
                }
                InitializationError::Stash(e) => {
                    let e = e.into();
                    error!("Failed to sync {e:?}");
                    Err(e)
                }
            },
            Ok(AsyncTaskResult::Cancelled) => {
                error!("Called while syncing {stage:?}");
                Err(MailContextError::TaskCancelled)
            }
            Err(e) => {
                let e = e.into();
                error!("Panicked while syncing {stage:?}: {e:?}");
                Err(e)
            }
        }
    }

    fn spawn_init<'a, T, F, Fut>(
        self: &'a Arc<Self>,
        watcher: &'a Arc<InitializationWatcher>,
        f: F,
    ) -> JoinHandle<AsyncTaskResult<T>>
    where
        T: Send + 'static,
        F: FnOnce(Arc<Self>, Arc<InitializationWatcher>) -> Fut,
        Fut: Future<Output = T> + Send + 'static,
    {
        let ctx_clone = self.clone();
        let watcher_clone = watcher.clone();
        self.spawn(f(ctx_clone, watcher_clone))
    }
}

/// Key used to distinguish between components in the initialization.
/// It is a string, not an enum for making it open for additional changes from different BU.
///
const EVENT_INIT_KEY: InitializationKey = InitializationKey::new("events");

async fn initialize_event_loop(
    watcher: Arc<InitializationWatcher>,
    ctx_clone: &MailUserContext,
) -> Result<(), InitializationError<EventLoopError>> {
    let stash = ctx_clone.user_stash();
    InitializedComponent::initialize::<EventLoopError, ()>(
        watcher,
        EVENT_INIT_KEY,
        &[],
        stash.connection(),
        async || {
            // This is a little bit of a hack. The way of how this
            // event loop initialization is currently written,
            // there is no way of initializing it with already having transaction.
            // We want to avoid the deadlock, and we do not depend on any dependencies.
            // So initializing it here is not really harmful, just weird.
            ctx_clone
                .event_loop
                .initialize(ctx_clone, ctx_clone)
                .await?;
            Ok(())
        },
        async |_tx, ()| Ok(()),
    )
    .await
}
