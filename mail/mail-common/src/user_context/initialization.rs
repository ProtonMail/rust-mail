use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::models::{ConversationCounters, MailSettings, MessageCounters};
use crate::{MailContextError, MailUserContext};
use futures::try_join;
use proton_api_core::session::CoreSession;
use proton_core_common::models::{Address, Contact, Label, User};
use tracing::{debug, error, trace, warn, Level};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MailUserContextLoadingStage {
    User,
    MailSettings,
    Addresses,
    Events,
    LabelsAndContacts,
    Contacts,
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
        #[tracing::instrument(level = Level::DEBUG, skip(fut, cb))]
        async fn initial_sync_for(
            stage: MailUserContextLoadingStage,
            fut: impl Future<Output = Result<(), MailContextError>> + Send + 'static,
            cb: &dyn MailUserContextInitializationCallback,
        ) -> Result<(), (MailUserContextLoadingStage, MailContextError)> {
            let t = Instant::now();
            debug!("Begin syncing for {stage:?}");

            let r = tokio::spawn(fut).await;
            let elapsed = t.elapsed();
            if elapsed > Duration::from_secs(1) {
                warn!("Slow sync for {stage:?}: {elapsed:?}");
            } else {
                debug!("Syncing {stage:?} took {elapsed:?}");
            }

            let t = Instant::now();
            cb.on_stage(stage);
            trace!("Callback took {:?}", t.elapsed());

            let r = match r {
                Ok(ok) => ok,
                Err(e) => Err(e.into()),
            };

            r.map_err(|e| {
                error!("Failed to sync {stage:?}: {e}");
                (stage, e)
            })
        }

        let ctx_clone = ctx.clone();
        let event_loop = async move {
            ctx_clone
                .exclusive
                .initialize_event_loop(ctx_clone.as_ref(), ctx_clone.as_ref())
                .await
                .map_err(MailContextError::from)
        };
        let ctx_clone = ctx.clone();
        let user_settings = async move {
            User::sync_user_and_settings(ctx_clone.session().api(), ctx_clone.user_stash())
                .await
                .map_err(MailContextError::from)
        };
        let ctx_clone = ctx.clone();
        let mail_settings = async move {
            MailSettings::sync_mail_settings(ctx_clone.session().api(), ctx_clone.user_stash())
                .await
                .map_err(MailContextError::from)
        };
        let ctx_clone = ctx.clone();
        let addresses = async move {
            Address::sync(ctx_clone.session().api(), ctx_clone.user_stash())
                .await
                .map_err(MailContextError::from)
        };
        let ctx_clone = ctx.clone();
        let labels_and_contacts = async move {
            let labels = Label::all_labels(ctx_clone.session().api()).await?;
            let mut tether = ctx_clone.user_stash().connection();
            let tx = tether.transaction().await?;
            let label_ids = Label::sync_labels(&tx, labels)
                .await
                .map_err(MailContextError::from)?;
            for local_id in label_ids {
                ConversationCounters::new(local_id).save(&tx).await?;
                MessageCounters::new(local_id).save(&tx).await?;
            }

            tx.commit().await?;

            // FIXME:(perf): This should be a different future that requests contact
            // group labels
            Contact::sync(ctx_clone.session().api(), ctx_clone.user_stash())
                .await
                .map_err(MailContextError::from)
        };

        try_join!(
            initial_sync_for(MailUserContextLoadingStage::Events, event_loop, cb),
            initial_sync_for(MailUserContextLoadingStage::User, user_settings, cb),
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
        Ok(())
    }
}
