pub mod attachments;

use crate::core::datatypes::Id;
use crate::errors::UserSessionError;
use crate::mail::datatypes::ViewMode;
use crate::mail::MailUserSession;
use crate::{uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use proton_api_core::services::proton::common::LabelId as RealLabelId;
use proton_api_core::services::proton::Proton;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::MailUserContext;
use stash::stash::Stash;
use std::sync::Arc;
use tracing::error;

/// A [`Mailbox`] provides a gateway to manipulating messages and conversations for a given label.
#[derive(uniffi::Object)]
pub struct Mailbox {
    /// The inner mailbox, which is the real internal type.
    mbox: proton_mail_common::Mailbox,
}

/// Callback for operations that get scheduled in the background and return no result.
#[uniffi::export(callback_interface)]
pub trait MailboxBackgroundResult: Send + Sync {
    fn on_background_result(&self, error: Option<UserSessionError>);
}

const DEFAULT_CONVERSATION_COUNT: usize = 50;

export_typed_result!(NewMailboxResult, Arc<Mailbox>, UserSessionError);

/// Create a new mailbox for a given label id.
#[uniffi::export]
pub async fn new_mailbox(ctx: &MailUserSession, label_id: Id) -> NewMailboxResult {
    let ctx = ctx.ctx().clone();
    uniffi_async(async move {
        let mbox = proton_mail_common::Mailbox::new(ctx, label_id.into()).await?;
        if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
            error!("Could not sync mailbox: {e:?}");
        }
        Result::<_, RealProtonMailError>::Ok(Arc::new(Mailbox { mbox }))
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}

/// Create a new mailbox for Inbox.
///
/// This mailbox will contain mail items from the Inbox alone, which is a
/// special system label.
///
/// # Parameters
///
/// * `ctx` - The mail user session. Note that this is a session that is
///           already authenticated and has a valid user context.
///
/// # Errors
///
/// Returns an error if the mailbox could not be created or synced.
///
#[uniffi::export]
pub async fn new_inbox_mailbox(ctx: &MailUserSession) -> NewMailboxResult {
    let ctx = ctx.ctx().clone();
    uniffi_async(async move {
        let mbox = proton_mail_common::Mailbox::with_remote_id(ctx, RealLabelId::inbox()).await?;

        Result::<_, RealProtonMailError>::Ok(Arc::new(Mailbox { mbox }))
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}

/// Create a new mailbox for all mail items.
///
/// This mailbox will contain all mail items, from all labels, using the
/// special system label "All Mail".
///
/// # Parameters
///
/// * `ctx` - The mail user session. Note that this is a session that is
///           already authenticated and has a valid user context.
///
/// # Errors
///
/// Returns an error if the mailbox could not be created or synced.
///
#[uniffi::export]
pub async fn new_all_mail_mailbox(ctx: &MailUserSession) -> NewMailboxResult {
    let ctx = ctx.ctx().clone();
    uniffi_async(async move {
        let mbox =
            proton_mail_common::Mailbox::with_remote_id(ctx, RealLabelId::all_mail()).await?;

        Result::<_, RealProtonMailError>::Ok(Arc::new(Mailbox { mbox }))
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}

#[proton_uniffi_macros::export_result]
impl Mailbox {
    /// Get the label id of the mailbox.
    #[must_use]
    pub fn label_id(&self) -> Id {
        self.mbox.label_id().into()
    }

    /// Get the mailbox's active view mode.
    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.mbox.view_mode().into()
    }

    /// Get the number of unread items in this mailbox.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn unread_count(&self) -> Result<u64, UserSessionError> {
        let mbox = self.mbox.clone();
        uniffi_async(
            async move { Result::<_, RealProtonMailError>::Ok(mbox.unread_count().await?) },
        )
        .await
        .map_err(UserSessionError::from)
    }

    /// Subscribe for updates to the number of unread items in this mailbox.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    #[allow(clippy::missing_panics_doc)]
    pub async fn watch_unread_count(
        &self,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, UserSessionError> {
        let mbox = self.mbox.clone();
        uniffi_async(async move {
            let receiver = mbox.watch_unread_count().await?;
            let watcher = watch_channel(mbox.user_context().as_ref(), receiver, callback);

            Result::<_, RealProtonMailError>::Ok(watcher)
        })
        .await
        .map_err(UserSessionError::from)
    }
}

/// Create a new mailbox for a given label id.
#[uniffi::export]
pub async fn with_label_id(ctx: &MailUserSession, label_id: Id) -> NewMailboxResult {
    // Note: This is a workaround for the default constructor not being able to be
    // generated on Kotlin.
    new_mailbox(ctx, label_id).await
}

impl Mailbox {
    /// Get the inner mailbox.
    #[must_use]
    pub fn mbox(&self) -> &proton_mail_common::Mailbox {
        &self.mbox
    }

    /// Get the API service.
    #[must_use]
    pub fn api(&self) -> &Proton {
        self.mbox.api()
    }

    /// Get the database connection.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        self.mbox.stash()
    }

    /// Get the [`MailUserContext`].
    #[must_use]
    pub fn context(&self) -> Arc<MailUserContext> {
        self.mbox.user_context()
    }
}
