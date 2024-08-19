use std::sync::Arc;

use crate::{spawn_async, uniffi_async, LiveQueryCallback, WatchHandle};
use proton_mail_common::models::MailSettings as RealSettings;
use stash::orm::Model;
use tokio::task::JoinError;

use super::{datatypes::MailSettings, MailSessionError, MailUserSession};

/// Gets the latest settings or a default if it can't find it.
#[uniffi::export]
pub async fn mail_settings(ctx: &MailUserSession) -> MailSettings {
    let stash = ctx.ctx().stash().clone();
    uniffi_async::<_, JoinError, _>(async move {
        Ok(RealSettings::get(&stash.into())
            .await
            .unwrap_or_default()
            .unwrap_or_default()
            .into())
    })
    .await
    .unwrap_or(MailSettings::default())
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
) -> Result<SettingsWatcher, MailSessionError> {
    let db = ctx.ctx().stash().clone();
    uniffi_async(async move {
        let (tx, rx) = flume::unbounded();
        let settings = RealSettings::find("", vec![], &db, Some(tx))
            .await?
            .first()
            .cloned()
            .unwrap_or_default()
            .into();
        let watch_handle = WatchHandle::new();

        spawn_async({
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
    })
    .await
}
