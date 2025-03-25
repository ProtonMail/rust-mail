use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::datatypes::InitializedComponentKey;
use crate::models::{
    InitializationError, InitializedComponent, LabelWithCounters, MailSettings, StoreLabelCounters,
};
use crate::{MailContextError, MailUserContext};
use futures::try_join;
use proton_api_core::services::proton::Proton;
use proton_core_common::CoreContextError;
use proton_core_common::async_task::AsyncTaskResult;
use proton_core_common::models::{
    Address, Contact, SyncedAddresses, SyncedContacts, SyncedUserSettings, User,
};
use proton_event_loop::EventLoopError;
use stash::stash::Stash;
use tokio::task::JoinHandle;
use tracing::{Level, debug, error, warn};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MailUserContextLoadingStage {
    UserSettings,
    MailSettings,
    Addresses,
    Events,
    Labels,
    Counters,
    Contacts,
    Finished,
}
pub trait MailUserContextInitializationCallback: Send + Sync + 'static {
    fn on_stage(&self, stage: MailUserContextLoadingStage);
    fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError);
}

impl MailUserContext {
    /// Initialize a component.
    #[tracing::instrument(level = Level::DEBUG, skip(handle, cb))]
    async fn initial_sync_for<
        E: Into<MailContextError> + std::fmt::Debug + Send + Sync + 'static,
    >(
        stage: MailUserContextLoadingStage,
        handle: JoinHandle<AsyncTaskResult<Result<(), InitializationError<E>>>>,
        cb: &dyn MailUserContextInitializationCallback,
    ) -> Result<(), (MailUserContextLoadingStage, MailContextError)> {
        let t = Instant::now();
        debug!("Begin syncing for {stage:?}");

        let result = handle.await;
        let elapsed = t.elapsed();
        if elapsed > Duration::from_secs(1) {
            warn!("Slow sync for {stage:?}: {elapsed:?}");
        } else {
            debug!("Syncing {stage:?} took {elapsed:?}");
        }

        cb.on_stage(stage);
        match result {
            Ok(AsyncTaskResult::Completed(Ok(()))) => Ok(()),
            Ok(AsyncTaskResult::Completed(Err(e))) => {
                let e = e.into();
                error!("Failed to sync {stage:?}: {e:?}");
                Err((stage, e))
            }
            Ok(AsyncTaskResult::Cancelled) => {
                error!("Called while syncing {stage:?}");
                Err((stage, MailContextError::TaskCancelled))
            }
            Err(e) => {
                let e = e.into();
                error!("Panicked while syncing {stage:?}: {e:?}");
                Err((stage, e))
            }
        }
    }

    /// Initialize the mail user context, running all the necessary syncs to ensure the context is ready to be used.
    /// Syncs are mostly run in the parallel, but updating message & conversation count are dependent on labels, so it is run in sequence.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The mail user context to initialize, it is vital to have it as Arc, as it will be cloned multiple times, and passed to the tokio::task.
    /// * `cb` - The callback to notify the caller about the progress of the initialization.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the initialization is successful.
    /// * `Err((MailUserContextLoadingStage, MailContextError))` - If the initialization fails at any stage, it will return the stage at which it failed and the error.
    ///
    #[tracing::instrument(level = Level::DEBUG, skip(ctx, cb))]
    pub async fn initialize_async(
        ctx: Arc<Self>,
        cb: &dyn MailUserContextInitializationCallback,
    ) -> Result<(), (MailUserContextLoadingStage, MailContextError)> {
        let t0 = Instant::now();
        let ctx_clone = ctx.clone();
        let labels = ctx.spawn(async move {
            LabelWithCounters::initialize(ctx_clone.api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let counters = ctx.spawn(async move {
            StoreLabelCounters::initialize(ctx_clone.api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let contacts =
            ctx.spawn(
                async move { initialize_contacts(ctx_clone.api(), ctx_clone.user_stash()).await },
            );

        let ctx_clone = ctx.clone();
        let event_loop = ctx.spawn(async move { initialize_event_loop(ctx_clone.as_ref()).await });
        let ctx_clone = ctx.clone();
        let user_settings = ctx.spawn(async move {
            initialize_user_settings(ctx_clone.api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let mail_settings = ctx.spawn(async move {
            MailSettings::initialize(ctx_clone.api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let addresses = ctx.spawn(async move {
            initialize_addresses(ctx_clone.api(), ctx_clone.user_stash()).await
        });

        try_join!(
            Self::initial_sync_for(MailUserContextLoadingStage::Labels, labels, cb),
            Self::initial_sync_for(MailUserContextLoadingStage::Contacts, contacts, cb),
            Self::initial_sync_for(MailUserContextLoadingStage::Counters, counters, cb),
            Self::initial_sync_for(MailUserContextLoadingStage::Events, event_loop, cb),
            Self::initial_sync_for(MailUserContextLoadingStage::UserSettings, user_settings, cb),
            Self::initial_sync_for(MailUserContextLoadingStage::MailSettings, mail_settings, cb),
            Self::initial_sync_for(MailUserContextLoadingStage::Addresses, addresses, cb),
        )?;

        debug!("Syncing Complete in {:?}", t0.elapsed());
        cb.on_stage(MailUserContextLoadingStage::Finished);

        Ok(())
    }
}

async fn initialize_contacts(
    api: &Proton,
    stash: &Stash,
) -> Result<(), InitializationError<CoreContextError>> {
    InitializedComponent::initialize::<CoreContextError, SyncedContacts>(
        InitializedComponentKey::Contacts,
        &[InitializedComponentKey::Labels],
        stash.connection(),
        async move || Contact::sync(api).await,
        async |tx, res| {
            res.store(tx).await?;
            Ok(())
        },
    )
    .await
}

async fn initialize_event_loop(
    ctx_clone: &MailUserContext,
) -> Result<(), InitializationError<EventLoopError>> {
    let stash = ctx_clone.user_stash();
    InitializedComponent::initialize::<EventLoopError, ()>(
        InitializedComponentKey::Events,
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

async fn initialize_user_settings(
    api: &Proton,
    stash: &Stash,
) -> Result<(), InitializationError<CoreContextError>> {
    InitializedComponent::initialize::<CoreContextError, SyncedUserSettings>(
        InitializedComponentKey::UserSettings,
        &[],
        stash.connection(),
        async move || User::sync_user_and_settings(api).await,
        async |tx, res| {
            res.store(tx).await?;
            Ok(())
        },
    )
    .await
}

async fn initialize_addresses(
    api: &Proton,
    stash: &Stash,
) -> Result<(), InitializationError<CoreContextError>> {
    InitializedComponent::initialize::<CoreContextError, SyncedAddresses>(
        InitializedComponentKey::Addresses,
        &[],
        stash.connection(),
        async move || Address::sync(api).await,
        async |tx, res| {
            res.store(tx).await?;
            Ok(())
        },
    )
    .await
}
