use std::sync::Arc;

use crate::{
    core::datatypes::AppSettings,
    errors::{ActionError, VoidActionResult},
    mail::MailUserSession,
    uniffi_async,
};
use proton_core_common::models::AppSettings as RealAppSettings;
use proton_mail_common::errors::ProtonMailError;

#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn change_app_settings(
    session: Arc<MailUserSession>,
    settings: AppSettings,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?.mail_context().core_context().clone();

    uniffi_async(async move {
        let mut tether = ctx.account_stash().connection();
        let real_app_settings = RealAppSettings::get_or_default(&tether).await;
        let mut real_app_settings = settings.merge_with_current(real_app_settings);
        let bond = tether.transaction().await?;

        real_app_settings.save(&bond).await?;
        bond.commit().await?;

        Result::<_, ProtonMailError>::Ok(())
    })
    .await
    .map_err(ActionError::from)
    .into()
}
