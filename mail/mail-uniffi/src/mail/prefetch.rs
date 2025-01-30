use std::sync::Arc;

use proton_mail_common::{errors::ProtonMailError as RealProtonMailError, prefetch::Prefetch};

use crate::{errors::ActionError, uniffi_async};

use super::MailUserSession;

#[proton_uniffi_macros::export_result]
async fn prefetch(session: Arc<MailUserSession>) -> Result<(), ActionError> {
    let ctx = session.ctx();
    uniffi_async(async move {
        Prefetch::key_locations(ctx).await;
        Result::<_, RealProtonMailError>::Ok(())
    })
    .await
    .map_err(Into::into)
}
