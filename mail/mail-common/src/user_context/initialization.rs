use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::{ConversationCounters, MailSettings, MessageCounters, StoreLabelCounters};
use crate::prefetch::Prefetch;
use crate::{MailContextError, MailUserContext};
use futures::try_join;
use proton_api_core::session::CoreSession;
use proton_core_common::models::{Address, Contact, Label, User};
use tokio::task::JoinHandle;
use tracing::{debug, error, warn, Level};

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
        #[tracing::instrument(level = Level::DEBUG, skip(handle, cb))]
        async fn initial_sync_for<E: Into<MailContextError> + Send + 'static>(
            stage: MailUserContextLoadingStage,
            handle: JoinHandle<Result<(), E>>,
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
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => {
                    let e = e.into();
                    error!("Failed to sync {stage:?}: {e}");
                    Err((stage, e))
                }
                Err(e) => {
                    let e = e.into();
                    error!("Panicked while syncing {stage:?}: {e}");
                    Err((stage, e))
                }
            }
        }

        let ctx_clone = ctx.clone();
        let event_loop = tokio::spawn(async move {
            ctx_clone
                .event_loop
                .initialize(ctx_clone.as_ref(), ctx_clone.as_ref())
                .await
        });
        let ctx_clone = ctx.clone();
        let user_settings = tokio::spawn(async move {
            User::sync_user_and_settings(ctx_clone.session().api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let mail_settings = tokio::spawn(async move {
            MailSettings::sync_mail_settings(ctx_clone.session().api(), ctx_clone.user_stash())
                .await
        });
        let ctx_clone = ctx.clone();
        let addresses = tokio::spawn(async move {
            Address::sync(ctx_clone.session().api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let labels_and_contacts = tokio::spawn(async move {
            let labels = Label::all_labels(ctx_clone.session().api()).await?;

            let api = ctx_clone.session().api().to_owned();
            let counters = tokio::spawn(async move { StoreLabelCounters::new(&api).await });
            let mut tether = ctx_clone.user_stash().connection();
            let tx = tether.transaction().await?;
            let label_ids = Label::sync_labels(&tx, labels).await?;
            for local_id in label_ids {
                ConversationCounters::new(local_id).save(&tx).await?;
                MessageCounters::new(local_id).save(&tx).await?;
            }

            tx.commit().await?;

            let counters = counters.await.expect("Can't fail to join")?;
            counters.store(ctx_clone.user_stash()).await?;
            // FIXME:(perf): This should be a different future that requests contact
            // group labels
            Contact::sync(ctx_clone.session().api(), ctx_clone.user_stash()).await?;
            Ok::<_, MailContextError>(())
        });

        try_join!(
            initial_sync_for(MailUserContextLoadingStage::Events, event_loop, cb),
            initial_sync_for(MailUserContextLoadingStage::UserSettings, user_settings, cb),
            initial_sync_for(MailUserContextLoadingStage::MailSettings, mail_settings, cb),
            initial_sync_for(MailUserContextLoadingStage::Addresses, addresses, cb),
            initial_sync_for(
                MailUserContextLoadingStage::LabelsAndContacts,
                labels_and_contacts,
                cb
            ),
        )?;

        debug!("Syncing Complete");
        cb.on_stage(MailUserContextLoadingStage::Finished);

        let (sender, receiver) = flume::unbounded();
        *ctx.prefetch.lock().await = Some(sender);
        Prefetch::initialize(ctx.clone(), receiver).await;

        Ok(())
    }
}
