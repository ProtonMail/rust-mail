use std::sync::Arc;

use proton_mail_common::errors::ProtonMailError;

use crate::{
    errors::{UserSessionError, VoidSessionResult},
    uniffi_async,
};

use super::MailUserSession;

/// Sing out from all accounts.
///
/// This method is going to remove all user data & account data
/// associated with the mail application.
///
/// This method is meant to be used when someone decides to sign out on
/// authentication screen such as PIN or Biometrics verification.
///
#[uniffi_export]
#[returns(VoidSessionResult)]
pub async fn sign_out_all(session: Arc<MailUserSession>) -> Result<(), UserSessionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        user_context.sign_out_all().await?;

        Result::<(), ProtonMailError>::Ok(())
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}
