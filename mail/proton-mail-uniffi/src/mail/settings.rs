use std::sync::Arc;

use proton_mail_common::models::MailSettings as RealSettings;
use stash::{orm::Model, stash::StashError};

use crate::{LiveQueryCallback, WatchHandle};

use super::{datatypes::MailSettings, MailUserSession};

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MailSettingsError {
    #[error("Database error: {0}")]
    DbError(#[from] StashError),
}

/// Gets the latest settings or a default if it can't find it.
#[uniffi::export]
pub async fn mail_settings(ctx: &MailUserSession) -> MailSettings {
    RealSettings::get(&ctx.ctx().stash().into())
        .await
        .unwrap_or_default()
        .unwrap_or_default()
        .into()
}

#[derive(Clone, uniffi::Record)]
pub struct SettingsWatcher {
    pub settings: MailSettings,
    pub watch_handle: Arc<WatchHandle>,
}

/// Calls on_update with the new mail settings every time the mail settings change.
#[uniffi::export]
pub async fn watch_mail_settings(
    ctx: &MailUserSession,
    on_update: Box<dyn LiveQueryCallback>,
) -> Result<SettingsWatcher, MailSettingsError> {
    let (tx, rx) = flume::unbounded();
    let db = ctx.ctx().stash().clone();
    let settings = RealSettings::find("", vec![], &db, Some(tx))
        .await?
        .first()
        .cloned()
        .unwrap_or_default()
        .into();
    let watch_handle = WatchHandle::new();

    _ = tokio::spawn({
        let watch_handle = watch_handle.clone();
        async move {
            while (rx.recv_async().await).is_ok() {
                if watch_handle.should_stop() {
                    break;
                }
                on_update.on_update();
            }
        }
    });

    Ok(SettingsWatcher {
        watch_handle: Arc::new(watch_handle),
        settings,
    })
}
