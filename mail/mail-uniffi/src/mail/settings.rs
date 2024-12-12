use std::sync::Arc;

use crate::errors::UserSessionError;
use crate::{uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::MailSettings as RealSettings;
use stash::orm::Model;
use tokio::task::JoinError;

use super::{datatypes::MailSettings, MailUserSession};

/// Gets the latest settings or a default if it can't find it.
#[uniffi::export]
pub async fn mail_settings(ctx: &MailUserSession) -> MailSettings {
    let stash = ctx.ctx().user_stash().clone();
    uniffi_async::<_, JoinError, _>(async move {
        let tether = stash.connection();
        Ok(RealSettings::get_or_default(&tether).await.into())
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
#[proton_uniffi_macros::export_result]
pub async fn watch_mail_settings(
    ctx: &MailUserSession,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<SettingsWatcher, UserSessionError> {
    let stash = ctx.ctx().user_stash().clone();
    uniffi_async(async move {
        let (tx, rx) = flume::unbounded();
        let tether = stash.connection();
        let settings = RealSettings::find("", vec![], &tether, Some(tx))
            .await?
            .first()
            .cloned()
            .unwrap_or_default()
            .into();

        let watcher = watch_channel(rx, callback).await;

        Result::<_, RealProtonMailError>::Ok(SettingsWatcher {
            watch_handle: watcher,
            settings,
        })
    })
    .await
    .map_err(UserSessionError::from)
}
