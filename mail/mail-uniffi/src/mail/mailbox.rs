pub mod attachments;

use crate::core::datatypes::Id;
use crate::errors::unexpected::UnexpectedError;
use crate::errors::{ProtonError, UserSessionError};
use crate::mail::MailUserSession;
use crate::mail::datatypes::{MessageRecipientDisplayMode, ViewMode};
use crate::mail::state::MailUserContextPtr;
use crate::{LiveQueryCallback, WatchHandle, declare_live_query_tagger, uniffi_async};
use proton_core_api::services::proton::LabelId as RealLabelId;
use proton_core_api::session::Session;
use proton_mail_common::MailUserContext;
use proton_mail_common::Mailbox as RealMailbox;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::datatypes::SystemLabelId;
use stash::stash::Stash;
use std::sync::Arc;
use uniffi_runtime::async_runtime;

/// A [`Mailbox`] provides a gateway to manipulating messages and conversations for a given label.
#[derive(uniffi::Object)]
pub struct Mailbox {
    /// The mail user context relevant for the mailbox.
    ctx: MailUserContextPtr,

    /// The real mailbox instance.
    mbox: RealMailbox,
}

impl Mailbox {
    /// Get a strong reference to the inner user context.
    pub(crate) fn ctx(&self) -> Result<Arc<MailUserContext>, ProtonError> {
        Ok(self.ctx.upgrade().ok_or(UnexpectedError::Internal)?)
    }

    /// Gets a weak mail user context pointer. Quickly clonable but does not
    /// guarantee that the context will be still alive.
    pub(crate) fn ctx_ptr(&self) -> MailUserContextPtr {
        self.ctx.clone()
    }

    /// Get the connection to the user database
    pub(crate) fn user_stash(&self) -> Result<Stash, ProtonError> {
        Ok(self.ctx()?.user_stash().to_owned())
    }
}

/// Callback for operations that get scheduled in the background and return no result.
#[uniffi::export(callback_interface)]
pub trait MailboxBackgroundResult: Send + Sync {
    fn on_background_result(&self, error: Option<UserSessionError>);
}

/// Create a new mailbox for a given label id.
#[uniffi_export]
pub fn new_mailbox(ctx: &MailUserSession, label_id: Id) -> Result<Arc<Mailbox>, UserSessionError> {
    let ptr = ctx.ptr();
    let ctx = ctx.ctx()?;

    async_runtime()
        .block_on(async move {
            let stash = ctx.user_stash();
            let tether = stash.connection().await?;
            let mbox = RealMailbox::new(&tether, label_id.into()).await?;

            Result::<_, RealProtonMailError>::Ok(Arc::new(Mailbox { ctx: ptr, mbox }))
        })
        .map_err(UserSessionError::from)
        .into()
}

/// Create a new mailbox for Inbox.
///
/// This mailbox will contain mail items from the Inbox alone, which is a
/// special system label.
///
#[uniffi_export]
pub fn new_inbox_mailbox(ctx: &MailUserSession) -> Result<Arc<Mailbox>, UserSessionError> {
    let ptr = ctx.ptr();
    let ctx = ctx.ctx()?;

    async_runtime()
        .block_on(async move {
            let stash = ctx.user_stash();
            let tether = stash.connection().await?;
            let mbox = RealMailbox::with_remote_id(&tether, RealLabelId::inbox()).await?;

            Result::<_, RealProtonMailError>::Ok(Arc::new(Mailbox { ctx: ptr, mbox }))
        })
        .map_err(UserSessionError::from)
        .into()
}

/// Create a new mailbox for all mail items.
///
/// This mailbox will contain all mail items, from all labels, using the
/// special system label "All Mail".
///
#[uniffi_export]
pub fn new_all_mail_mailbox(ctx: &MailUserSession) -> Result<Arc<Mailbox>, UserSessionError> {
    let ptr = ctx.ptr();
    let ctx = ctx.ctx()?;

    async_runtime()
        .block_on(async move {
            let stash = ctx.user_stash();
            let tether = stash.connection().await?;
            let mbox = RealMailbox::with_remote_id(&tether, RealLabelId::all_mail()).await?;

            Result::<_, RealProtonMailError>::Ok(Arc::new(Mailbox { ctx: ptr, mbox }))
        })
        .map_err(UserSessionError::from)
        .into()
}

#[uniffi_export]
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

    #[must_use]
    pub fn recipient_display_mode(&self) -> MessageRecipientDisplayMode {
        self.mbox.recipient_display_mode().into()
    }

    /// Get the number of unread items in this mailbox.
    pub async fn unread_count(&self) -> Result<u64, UserSessionError> {
        let stash = self.user_stash()?;
        let mbox = self.mbox.clone();

        uniffi_async(async move {
            let tether = stash.connection().await?;
            let count = mbox.unread_count(&tether).await?;

            Result::<_, RealProtonMailError>::Ok(count)
        })
        .await
        .map_err(UserSessionError::from)
    }

    /// Subscribe for updates to the number of unread items in this mailbox.
    pub async fn watch_unread_count(
        &self,
        callback: Box<dyn LiveQueryCallback>,
    ) -> Result<Arc<WatchHandle>, UserSessionError> {
        let ctx = self.ctx()?;
        let mbox = self.mbox.clone();

        uniffi_async(async move {
            let receiver = mbox.watch_unread_count(ctx.user_stash()).await?;
            let watcher = WatchUnreadCounterMarker::watch_channel(&*ctx, receiver, callback);

            Result::<_, RealProtonMailError>::Ok(watcher)
        })
        .await
        .map_err(UserSessionError::from)
    }
}

declare_live_query_tagger!(WatchUnreadCounterMarker);

impl Mailbox {
    /// Get the inner mailbox.
    #[must_use]
    pub fn mbox(&self) -> &RealMailbox {
        &self.mbox
    }

    /// Get the API session.
    pub fn session(&self) -> Result<Session, ProtonError> {
        Ok(self.ctx()?.session().to_owned())
    }

    /// Get the database connection.
    pub fn stash(&self) -> Result<Stash, ProtonError> {
        Ok(self.ctx()?.user_stash().to_owned())
    }
}
