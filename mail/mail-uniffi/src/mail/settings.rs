use std::sync::Arc;

use crate::errors::UserSessionError;
use crate::{LiveQueryCallback, WatchHandle, uniffi_async, watch_channel};
use proton_core_common::models::ModelExtension;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::models::MailSettings as RealSettings;
use tokio::task::JoinError;

use super::{MailUserSession, datatypes::MailSettings};

/// Gets the latest settings or a default if it can't find it.
#[uniffi_export]
pub async fn mail_settings(ctx: &MailUserSession) -> Result<MailSettings, UserSessionError> {
    let stash = ctx.user_stash()?;
    Ok(uniffi_async::<_, JoinError, _>(async move {
        let tether = stash.connection();
        Ok(RealSettings::get_or_default(&tether).await.into())
    })
    .await
    .unwrap_or(MailSettings::default()))
}

#[derive(Clone, uniffi::Record)]
pub struct SettingsWatcher {
    pub settings: MailSettings,
    pub watch_handle: Arc<WatchHandle>,
}

/// Calls on_update with the new mail settings every time the mail settings change.
#[uniffi_export]
pub async fn watch_mail_settings(
    ctx: &MailUserSession,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<SettingsWatcher, UserSessionError> {
    let ctx = ctx.ctx()?;
    uniffi_async(async move {
        let stash = ctx.user_stash();
        let tether = stash.connection();
        let settings = RealSettings::all(&tether)
            .await?
            .pop()
            .unwrap_or_default()
            .into();

        let handle = RealSettings::watch(stash)?;
        let watcher = watch_channel(ctx, handle, callback);

        Result::<_, RealProtonMailError>::Ok(SettingsWatcher {
            watch_handle: watcher,
            settings,
        })
    })
    .await
    .map_err(UserSessionError::from)
}
