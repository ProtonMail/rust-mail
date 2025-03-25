use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::{LabelWithCounters, MailSettings, StoreLabelCounters};
use crate::{MailContextError, MailUserContext};
use futures::future::try_join;
use futures::try_join;
use proton_core_common::async_task::AsyncTaskResult;
use proton_core_common::models::{Address, Contact, User};
use tokio::task::JoinHandle;
use tracing::{Level, debug, error, warn};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MailUserContextLoadingStage {
    UserSettings,
    MailSettings,
    Addresses,
    Events,
    /// TODO: Split into Labels and Contacts
    LabelsAndContacts,
    Counters,
    Finished,
}
pub trait MailUserContextInitializationCallback: Send + Sync + 'static {
    fn on_stage(&self, stage: MailUserContextLoadingStage);
    fn on_stage_err(&self, stage: MailUserContextLoadingStage, err: MailContextError);
}

impl MailUserContext {
    /// Initialize a component.
    #[tracing::instrument(level = Level::DEBUG, skip(handle, cb))]
    async fn initial_sync_for<E: Into<MailContextError> + Send + 'static>(
        stage: MailUserContextLoadingStage,
        handle: JoinHandle<AsyncTaskResult<Result<(), E>>>,
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
        let labels_and_contacts = ctx.spawn(async move {
            // Since contact syncing is the slowest part let's leave it downloading in parallel while we
            // setup all of the rest of the futures.
            let ctx_clone2 = ctx_clone.clone();
            let contact_sync_dl_fut = ctx_clone.spawn(async move {
                Contact::sync(ctx_clone2.api(), ctx_clone2.user_stash()).await
            });

            // let labels = Label::all_labels(ctx_clone.api()).await?;

            let api = ctx_clone.api().to_owned();
            let counters = ctx_clone.spawn(async move { StoreLabelCounters::new(&api).await });
            let mut tether = ctx_clone.user_stash().connection();
            let tx = tether.transaction().await?;
            LabelWithCounters::initialize(ctx_clone.api(), &tx).await?;

            tx.commit().await?;

            let (contact_fut, counters) = try_join(contact_sync_dl_fut, counters)
                .await
                .expect("Can't fail to join");

            // FIXME:(perf): This should be a different future that requests contact
            // group labels
            match contact_fut {
                AsyncTaskResult::Completed(f) => f?.await?,
                AsyncTaskResult::Cancelled => return Err(MailContextError::TaskCancelled),
            }
            match counters {
                AsyncTaskResult::Completed(counters) => {
                    counters?.store(ctx_clone.user_stash()).await?
                }
                AsyncTaskResult::Cancelled => return Err(MailContextError::TaskCancelled),
            }

            Ok::<_, MailContextError>(())
        });

        let ctx_clone = ctx.clone();
        let event_loop = ctx.spawn(async move {
            ctx_clone
                .event_loop
                .initialize(ctx_clone.as_ref(), ctx_clone.as_ref())
                .await
        });
        let ctx_clone = ctx.clone();
        let user_settings = ctx.spawn(async move {
            User::sync_user_and_settings(ctx_clone.api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let mail_settings = ctx.spawn(async move {
            MailSettings::sync_mail_settings(ctx_clone.api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let addresses =
            ctx.spawn(async move { Address::sync(ctx_clone.api(), ctx_clone.user_stash()).await });

        try_join!(
            Self::initial_sync_for(
                MailUserContextLoadingStage::LabelsAndContacts,
                labels_and_contacts,
                cb
            ),
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
