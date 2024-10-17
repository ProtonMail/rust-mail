use proton_core_common::models::Contact as RealContact;
use std::sync::Arc;

use crate::{core::datatypes::GroupedContacts, uniffi_async};

use super::{MailUserSession, MailboxError};

/// Returns grouped contacts by the first grapheme of the name.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn contact_list(
    session: Arc<MailUserSession>,
) -> Result<Vec<GroupedContacts>, MailboxError> {
    uniffi_async(async move {
        Ok(RealContact::contact_list(session.user_stash())
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    })
    .await
}
