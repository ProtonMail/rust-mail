use std::sync::Arc;

use crate::models::{Conversation, Label, MailSettings};
use crate::{MailContextError, MailContextResult, MailUserContext};
use proton_api_core::session::CoreSession;
use proton_core_common::models::{Address, Contact, User};
use tracing::{debug, error, Level};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MailUserContextLoadingStage {
    User,
    MailSettings,
    Addresses,
    Events,
    Labels,
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
    #[tracing::instrument(level = Level::DEBUG, skip(ctx, cb), fields(user_id=?ctx.user_id()))]
    pub async fn initialize_async(
        ctx: Arc<Self>,
        cb: &dyn MailUserContextInitializationCallback,
    ) -> Result<(), (MailUserContextLoadingStage, MailContextError)> {
        let ctx_clone = ctx.clone();
        let event_loop_handle = tokio::spawn(async move {
            debug!("Syncing event id");
            ctx_clone
                .exclusive
                .initialize_event_loop(ctx_clone.as_ref(), ctx_clone.as_ref())
                .await
        });
        let ctx_clone = ctx.clone();
        let user_settings_handle = tokio::spawn(async move {
            debug!("Syncing User settings");
            User::sync_user_and_settings(ctx_clone.session().api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let mail_settings_handle = tokio::spawn(async move {
            debug!("Syncing Mail settings");
            MailSettings::sync_mail_settings(ctx_clone.session().api(), ctx_clone.user_stash())
                .await
        });
        let ctx_clone = ctx.clone();
        let addresses_handle = tokio::spawn(async move {
            debug!("Syncing Addresses");
            Address::sync(ctx_clone.session().api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let labels_handle = tokio::spawn(async move {
            debug!("Syncing labels");
            Label::sync_labels(ctx_clone.session().api(), ctx_clone.user_stash()).await
        });
        let ctx_clone = ctx.clone();
        let contacts_handle = tokio::spawn(async move {
            debug!("Syncing contacts");
            Contact::sync(ctx_clone.session().api(), ctx_clone.user_stash()).await
        });

        let (
            event_loop_handle,
            user_settings_handle,
            mail_settings_handle,
            addresses_handle,
            labels_handle,
            contacts_handle,
        ) = tokio::join!(
            event_loop_handle,
            user_settings_handle,
            mail_settings_handle,
            addresses_handle,
            labels_handle,
            contacts_handle
        );
        let sync_count_result = Conversation::sync_conversation_and_message_counts(
            ctx.session().api(),
            ctx.user_stash(),
        )
        .await;

        fn map_err<T, E1, E2>(res: Result<Result<T, E1>, E2>) -> MailContextResult<T>
        where
            E1: Into<MailContextError>,
            E2: Into<MailContextError>,
        {
            res.map_err(|e| e.into())
                .and_then(|r| r.map_err(|e| e.into()))
        }

        let (
            sync_event_loop_result,
            sync_user_settings_result,
            sync_mail_settings_result,
            sync_addresses_result,
            sync_labels_result,
            sync_contacts_result,
        ) = (
            map_err(event_loop_handle),
            map_err(user_settings_handle),
            map_err(mail_settings_handle),
            map_err(addresses_handle),
            map_err(labels_handle),
            map_err(contacts_handle),
        );

        debug!("Syncing Complete");
        debug!("Validate event id");
        cb.on_stage(MailUserContextLoadingStage::Events);
        if let Err(e) = sync_event_loop_result {
            error!("Failed to sync event id:{e}");
            return Err((MailUserContextLoadingStage::Events, e));
        }

        debug!("Validate user settings");
        cb.on_stage(MailUserContextLoadingStage::User);
        if let Err(e) = sync_user_settings_result {
            error!("Failed to sync user settings: {e}");
            return Err((MailUserContextLoadingStage::User, e));
        }

        debug!("Validate mail settings");
        cb.on_stage(MailUserContextLoadingStage::MailSettings);
        if let Err(e) = sync_mail_settings_result {
            error!("Failed to sync user settings: {e}");
            return Err((MailUserContextLoadingStage::MailSettings, e));
        }

        debug!("Validate addresses");
        cb.on_stage(MailUserContextLoadingStage::Addresses);
        if let Err(e) = sync_addresses_result {
            error!("Failed to sync addresses :{e}");
            return Err((MailUserContextLoadingStage::Addresses, e));
        }

        debug!("Validate labels");
        cb.on_stage(MailUserContextLoadingStage::Labels);
        if let Err(e) = sync_labels_result {
            error!("Failed to sync labels: {e}");
            return Err((MailUserContextLoadingStage::Labels, e));
        }

        debug!("Validate contacts");
        cb.on_stage(MailUserContextLoadingStage::Contacts);
        if let Err(e) = sync_contacts_result {
            error!("Failed to sync contacts: {e}");
            return Err((MailUserContextLoadingStage::Contacts, e));
        }

        debug!("Validate conversation and message counts");
        cb.on_stage(MailUserContextLoadingStage::Counters);
        if let Err(e) = sync_count_result {
            error!("Failed to sync conversation and messages counter: {e}");
            return Err((MailUserContextLoadingStage::Counters, e.into()));
        }

        debug!("Validation complete");
        cb.on_stage(MailUserContextLoadingStage::Finished);
        Ok(())
    }
}
