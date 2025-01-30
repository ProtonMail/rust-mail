use std::sync::Arc;

use proton_mail_common::{errors::ProtonMailError as RealProtonMailError, prefetch::Prefetch};

use crate::{errors::ActionError, uniffi_async};

use super::MailUserSession;

/// Perform prefetching of messages and conversations in the background
/// for key locations agreed upon in et-offline-mode channel.
///
/// This function spawns a background task that will prefetch messages and conversations
/// The bacground task will be spawned only once. Subsequent calls to this function will
/// notify the task to start prefetching but the task will be executed only once per whole cycle.
#[proton_uniffi_macros::export_result]
async fn prefetch(_session: Arc<MailUserSession>) -> Result<(), ActionError> {
    uniffi_async(async move {
        Prefetch::key_locations();
        Result::<_, RealProtonMailError>::Ok(())
    })
    .await
    .map_err(Into::into)
}
