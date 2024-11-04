use proton_core_common::models::Contact as RealContact;
use proton_mail_common::MailContextError;
use std::sync::Arc;

use crate::{
    core::datatypes::{GroupedContacts, Id},
    uniffi_async,
};

use super::{MailSessionError, MailUserSession, MailboxError};

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

#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn delete_contact(
    contact_id: Id,
    session: Arc<MailUserSession>,
) -> Result<(), MailSessionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealContact::action_delete(
            user_context.session(),
            user_context.queue(),
            vec![contact_id.into()],
        )
        .await
        .map_err(MailContextError::from)?;

        Ok(())
    })
    .await
}
