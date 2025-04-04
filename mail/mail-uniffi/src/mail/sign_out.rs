use std::sync::Arc;

use proton_mail_common::{MailContextError, errors::ProtonMailError};

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
        let all_ctxs = user_context.all_mail_user_ctxs().await?;

        for ctx in all_ctxs {
            ctx.delete_account().await?;
        }

        user_context
            .mail_context()
            .core_context()
            .tear_down_account_database()
            .await
            .map_err(MailContextError::from)?;

        Result::<(), ProtonMailError>::Ok(())
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}
