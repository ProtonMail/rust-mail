use crate::core::datatypes::Id;
use crate::errors::UserSessionError;
use crate::mail::datatypes::TrackerInfo;
use crate::mail::user_session::MailUserSession;
use crate::{AsyncLiveQueryCallback, WatchHandle, declare_live_query_tagger, uniffi_async};
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::TrackerDetector;
use proton_mail_common::datatypes::LocalMessageId;
use proton_mail_common::models::{MessageTracker, MessageTrackerUrl};
use sqlite_watcher::watcher::TableObserver;
use stash::orm::Model;
use std::collections::BTreeSet;
use std::sync::Arc;

declare_live_query_tagger!(WatchTrackerInfoMarker);

#[derive(Clone, uniffi::Record)]
pub struct WatchedTrackerInfo {
    pub tracker_info: Option<TrackerInfo>,
    pub watch_handle: Arc<WatchHandle>,
}

#[uniffi_export]
pub async fn get_tracker_info_for_message(
    session: &MailUserSession,
    message_id: Id,
) -> Result<Option<TrackerInfo>, UserSessionError> {
    let stash = session.user_stash()?;

    uniffi_async::<_, RealProtonMailError, _>(async move {
        let tether = stash.connection().await?;
        Ok(
            TrackerDetector::get_tracker_info(message_id.into(), &tether)
                .await?
                .map(Into::into),
        )
    })
    .await
    .map_err(UserSessionError::from)
}

#[uniffi_export]
pub async fn watch_tracker_info_for_message(
    session: &MailUserSession,
    message_id: Id,
    callback: Arc<dyn AsyncLiveQueryCallback>,
) -> Result<WatchedTrackerInfo, UserSessionError> {
    let ctx = session.ctx()?;
    let stash = session.user_stash()?;

    uniffi_async(async move {
        let tether = stash.connection().await?;

        let info = TrackerDetector::get_tracker_info(message_id.into(), &tether).await?;

        let local_message_id: LocalMessageId = message_id.into();
        let handle = tether.subscribe_to(move |sender| {
            Box::new(MessageTrackerObserver {
                message_id: local_message_id,
                sender,
            })
        })?;

        let watch_handle = WatchTrackerInfoMarker::watch_channel_async(&*ctx, handle, callback);

        Ok::<_, RealProtonMailError>(WatchedTrackerInfo {
            tracker_info: info.map(Into::into),
            watch_handle,
        })
    })
    .await
    .map_err(UserSessionError::from)
}

struct MessageTrackerObserver {
    message_id: LocalMessageId,
    sender: flume::Sender<()>,
}

impl TableObserver for MessageTrackerObserver {
    fn tables(&self) -> Vec<String> {
        vec![
            MessageTracker::table_name().to_string(),
            MessageTrackerUrl::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for TrackerInfoTableObserver (message_id={}): {:?}",
                    self.message_id,
                    e
                );
            })
            .ok();
    }
}
