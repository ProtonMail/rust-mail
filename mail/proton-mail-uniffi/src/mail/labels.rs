//! Functions for working with [`Label`]s.
//!
//! The functions presented here can operate in one of two scopes: either on a
//! [`Mailbox`], or on a [`MailSession`]. The difference is that operations that
//! rely on the context of a mailbox/label view are performed on a mailbox, and
//! operations that are more global in nature are performed on a session. The
//! scope of the methods might change over time, but their primary association
//! of working with labels, and hence their placement in this module, won't.
//!

use crate::mail::datatypes::Label;
use crate::mail::{MailSession, MailboxError};
use crate::{LiveQueryCallback, WatchHandle};
use proton_mail_common::datatypes::LabelType as RealLabelType;
use proton_mail_common::models::Label as RealLabel;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::spawn as spawn_async;
use tracing::{debug, warn};

/// Messages and watch handle for watched labels.
#[derive(uniffi::Record)]
pub struct WatchedLabels {
    /// The labels being watched.
    labels: Vec<Label>,

    /// The handle to stop watching the labels.
    handle: Arc<WatchHandle>,
}

/// Watch labels of a given type.
///
/// Watches labels of a specified label type for changes. When the labels
/// change, the callback will be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `callback` - The callback to use for updates. When the specified label
///                list changes, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
async fn watch_labels(
    session: Arc<MailSession>,
    label_type: RealLabelType,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedLabels, MailboxError> {
    let (sender, receiver) = flume::unbounded::<ResultsetChange<RealLabel, u64>>();
    let results = RealLabel::find(
        "WHERE label_type = ?",
        params![label_type],
        session.stash(),
        Some(sender),
    )
    .await?;
    // Unwrapping is safe here, as we will always have the local ID
    let mut ids = results
        .iter()
        .map(|m| m.local_id.unwrap())
        .collect::<Vec<_>>();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = Arc::clone(&stop_flag);

    spawn_async(async move {
        while let Ok(change) = receiver.recv_async().await {
            if stop_flag_clone.load(Ordering::SeqCst) {
                debug!("Stop flag set, stopping watch");
                break;
            }
            match change {
                ResultsetChange::Inserted(label) => {
                    if label.label_type == label_type {
                        debug!("Received new label for watched label type ({label_type})");
                        // Unwrapping is safe here, as we will always have the local ID
                        ids.push(label.local_id.unwrap());
                        callback.on_update();
                    } else {
                        debug!("Received new label for different label type ({} instead of {label_type})", label.label_type);
                    }
                }
                ResultsetChange::Updated(label) => {
                    if label.label_type == label_type {
                        debug!("Received updated label for watched label type ({label_type})");
                        callback.on_update();
                    } else {
                        debug!("Received updated label for different label type ({} instead of {label_type})", label.label_type);
                    }
                }
                ResultsetChange::Deleted(local_label_id) => {
                    if ids.contains(&local_label_id) {
                        debug!("Received deleted label for watched label type ({label_type})");
                        callback.on_update();
                    } else {
                        debug!("Received deleted label for different label type (unknown instead of {label_type})");
                    }
                }
                _ => {
                    warn!("Received unknown change type");
                }
            };
        }
    });

    Ok(WatchedLabels {
        labels: results.into_iter().map(Into::into).collect(),
        handle: Arc::new(WatchHandle { stop_flag }),
    })
}

/// Watch folder labels.
///
/// Watches folder labels for changes. When the labels change, the callback will
/// be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `callback` - The callback to use for updates. When the specified label
///                list changes, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn watch_folder_labels(
    session: Arc<MailSession>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedLabels, MailboxError> {
    watch_labels(session, RealLabelType::Folder, callback).await
}

/// Watch standard labels.
///
/// Watches standard labels for changes. When the labels change, the callback will
/// be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `callback` - The callback to use for updates. When the specified label
///                list changes, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn watch_standard_labels(
    session: Arc<MailSession>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedLabels, MailboxError> {
    watch_labels(session, RealLabelType::Label, callback).await
}

/// Watch system labels.
///
/// Watches system labels for changes. When the labels change, the callback will
/// be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `callback` - The callback to use for updates. When the specified label
///                list changes, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn watch_system_labels(
    session: Arc<MailSession>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedLabels, MailboxError> {
    watch_labels(session, RealLabelType::System, callback).await
}
