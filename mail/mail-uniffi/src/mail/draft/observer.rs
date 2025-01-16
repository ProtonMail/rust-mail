use crate::core::datatypes::Id;
use crate::errors::{DraftError, VoidDraftResult};
use crate::mail::MailUserSession;
use crate::{async_runtime, uniffi_async};
use proton_mail_common::datatypes::LocalMessageId;
use proton_mail_common::draft::observers::DraftSendResultWatcher as RealDraftSendResultWatcher;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::{
    DraftSendResult as RealDraftSendResult, DraftSendResultOrigin as RealDraftSendResultOrigin,
};
use proton_mail_common::MailContextError;
use std::sync::Arc;
use tokio::task::AbortHandle;
use tracing::error;

/// Origin of the result.
#[derive(uniffi::Enum)]
pub enum DraftSendResultOrigin {
    /// When saving a draft without the intention to send.
    Save,
    /// When saving a draft with intention to send.
    SaveBeforeSend,
    /// Sending of the saved draft.
    Send,
}

impl From<RealDraftSendResultOrigin> for DraftSendResultOrigin {
    fn from(value: RealDraftSendResultOrigin) -> Self {
        match value {
            RealDraftSendResultOrigin::Save => Self::Save,
            RealDraftSendResultOrigin::SaveBeforeSend => Self::SaveBeforeSend,
            RealDraftSendResultOrigin::Send => Self::Send,
        }
    }
}

/// Indicates how a draft operation completed.
#[derive(uniffi::Enum)]
pub enum DraftSendStatus {
    /// Everything was completed with success.
    Success,
    /// Something failed.
    Failure(DraftError),
}

/// Result of sending a draft
#[derive(uniffi::Record)]
pub struct DraftSendResult {
    //TODO(ET-1843): Undo send token from remote id if applicable.
    /// The id of draft message
    pub message_id: Id,
    /// Timestamp at which the operation recorded
    pub timestamp: i64,
    /// Success or failure status.
    pub error: DraftSendStatus,
    /// Where this report originated from.
    pub origin: DraftSendResultOrigin,
}

impl From<RealDraftSendResult> for DraftSendResult {
    fn from(value: RealDraftSendResult) -> Self {
        Self {
            message_id: value.local_message_id.into(),
            timestamp: value.timestamp,
            error: value.error.map_or(DraftSendStatus::Success, |e| {
                DraftSendStatus::Failure(DraftError::from(RealProtonMailError::from(e)))
            }),
            origin: value.origin.into(),
        }
    }
}

/// Callback interface to be notified of new draft send results.
#[uniffi::export(with_foreign)]
pub trait DraftSendResultCallback: Send + Sync {
    /// Will be invoked with a list of at least 1 new send result.
    fn on_new_send_result(&self, details: Vec<DraftSendResult>);
}

export_typed_result!(
    NewDraftSendResultWatcherResult,
    Arc<DraftSendResultWatcher>,
    DraftError
);

/// Observe draft send results.
///
/// Note that this will only notify you of new records that have not been seen before.
#[derive(uniffi::Object)]
pub struct DraftSendResultWatcher {
    abort_handle: AbortHandle,
}

/// Create new instance of the watcher for the `session` with `callback`.
#[uniffi::export]
pub async fn new_draft_send_watcher(
    session: Arc<MailUserSession>,
    callback: Arc<dyn DraftSendResultCallback>,
) -> NewDraftSendResultWatcherResult {
    uniffi_async(async move {
        let ctx = session.ctx();
        let mut observer = RealDraftSendResultWatcher::new(ctx.user_stash().clone()).await?;
        let handle = async_runtime()
            .spawn(async move {
                loop {
                    match observer.next().await {
                        Ok(results) => {
                            let callback = callback.clone();
                            async_runtime().spawn_blocking(move || {
                                callback.on_new_send_result(
                                    results.into_iter().map(Into::into).collect(),
                                );
                            });
                        }
                        Err(e) => {
                            error!("Draft Send Result observer error: {:?}", e);
                            return;
                        }
                    }
                }
            })
            .abort_handle();
        Ok::<_, RealProtonMailError>(Arc::new(DraftSendResultWatcher {
            abort_handle: handle,
        }))
    })
    .await
    .map_err(DraftError::from)
    .into()
}

#[uniffi::export]
impl DraftSendResultWatcher {
    /// Disconnect the watcher and stop observing the table.
    pub fn disconnect(&self) {
        self.abort_handle.abort();
    }
}

/// Return all unseen send results for drafts.
///
/// # Errors
///
/// Returns error if the query failed.
#[proton_uniffi_macros::export_result]
pub async fn draft_send_result_unseen(
    session: &MailUserSession,
) -> Result<Vec<DraftSendResult>, DraftError> {
    let ctx = session.ctx();
    uniffi_async(async move {
        let connection = ctx.user_stash().connection();
        RealDraftSendResult::unseen(&connection)
            .await
            .map(|v| v.into_iter().map(Into::into).collect())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(DraftError::from)
}

/// Mark the send results for the `message_ids` as seen.
///
/// # Errors
///
/// Returns error if the query failed.
#[uniffi::export]
pub async fn draft_send_result_mark_seen(
    session: &MailUserSession,
    message_ids: Vec<Id>,
) -> VoidDraftResult {
    let ctx = session.ctx();
    uniffi_async(async move {
        let mut connection = ctx.user_stash().connection();
        let tx = connection.transaction().await?;
        RealDraftSendResult::mark_seen(message_ids.into_iter().map(LocalMessageId::from), &tx)
            .await?;
        tx.commit().await?;
        Ok(())
    })
    .await
    .map_err(|e: MailContextError| DraftError::from(RealProtonMailError::from(e)))
    .into()
}

/// Delete the send results for the `message_ids`.
///
/// # Errors
///
/// Returns error if the query failed.
#[uniffi::export]
pub async fn draft_send_result_delete(
    session: &MailUserSession,
    message_ids: Vec<Id>,
) -> VoidDraftResult {
    let ctx = session.ctx();
    uniffi_async(async move {
        let mut connection = ctx.user_stash().connection();
        let tx = connection.transaction().await?;
        RealDraftSendResult::delete(message_ids.into_iter().map(LocalMessageId::from), &tx).await?;
        tx.commit().await?;
        Ok(())
    })
    .await
    .map_err(|e: MailContextError| DraftError::from(RealProtonMailError::from(e)))
    .into()
}
