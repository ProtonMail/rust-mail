pub mod attachments;

use crate::core::datatypes::Id;
use crate::errors::user_session::UserSessionError;
use crate::mail::datatypes::ViewMode;
use crate::mail::MailUserSession;
use crate::{uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use proton_api_core::services::proton::Proton;
use proton_core_common::datatypes::LabelId as RealLabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::errors::user_session::{Reason, UserSessionError as RealUserSessionError};
use proton_mail_common::models::Label as RealLabel;
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

#[proton_uniffi_macros::export_result]
impl Mailbox {
    /// Create a new mailbox for a given label id.
    #[uniffi::constructor]
    pub async fn new(
        ctx: &MailUserSession,
        label_id: Id,
    ) -> Result<Arc<Mailbox>, UserSessionError> {
        let ctx = ctx.ctx().clone();
        uniffi_async(async move {
            let mbox = proton_mail_common::Mailbox::new(ctx, label_id.into()).await?;
            if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
                error!("Could not sync mailbox: {e}");
            }
            Result::<_, RealUserSessionError>::Ok(Arc::new(Self { mbox }))
        })
        .await
        .map_err(Into::into)
    }

    /// Create a new mailbox for Inbox.
    #[uniffi::constructor]
    pub async fn inbox(ctx: &MailUserSession) -> Result<Arc<Mailbox>, UserSessionError> {
        let ctx = ctx.ctx().clone();
        uniffi_async(async move {
            let mbox =
                proton_mail_common::Mailbox::with_remote_id(ctx, RealLabelId::inbox()).await?;
            Self::sync(mbox).await
        })
        .await
        .map_err(Into::into)
    }

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
            async move { Result::<_, RealUserSessionError>::Ok(mbox.unread_count().await?) },
        )
        .await
        .map_err(Into::into)
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
        let label_id = self.mbox.label_id();
        let stash = self.mbox.user_context().user_stash().clone();
        uniffi_async(async move {
            let Some((_, receiver)) = RealLabel::watch(label_id, &stash).await? else {
                return Err(Reason::UnknownLabel.into());
            };

            let watcher = watch_channel(receiver, callback);
            Result::<_, RealUserSessionError>::Ok(watcher)
        })
        .await
        .map_err(Into::into)
    }
}

#[uniffi::export]
impl Mailbox {
    /// Create a new mailbox for a given label id.
    #[uniffi::constructor]
    pub async fn with_label_id(ctx: &MailUserSession, label_id: Id) -> MailboxNewResult {
        // Note: This is a workaround for the default constructor not being able to be
        // generated on Kotlin.
        Self::new(ctx, label_id).await
    }
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

    async fn sync(mbox: proton_mail_common::Mailbox) -> Result<Arc<Self>, RealUserSessionError> {
        if let Err(e) = mbox.sync(DEFAULT_CONVERSATION_COUNT).await {
            error!("Could not sync mailbox: {e}");
        }
        Ok(Arc::new(Self { mbox }))
    }
}
