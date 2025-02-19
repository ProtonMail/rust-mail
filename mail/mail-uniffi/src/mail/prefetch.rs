use std::sync::Arc;

use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

use crate::{errors::ActionError, uniffi_async};

use super::MailUserSession;

/// Perform prefetching of messages and conversations in the background
/// for key locations agreed upon in et-offline-mode channel.
///
/// This function spawns a background task that will prefetch messages and conversations
/// The bacground task will be spawned only once. Subsequent calls to this function will
/// notify the task to start prefetching but the task will be executed only once per whole cycle.
#[uniffi_export]
async fn prefetch(session: Arc<MailUserSession>) -> Result<(), ActionError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        ctx.prefetch().await?;
        Result::<_, RealProtonMailError>::Ok(())
    })
    .await
    .map_err(Into::into)
}
