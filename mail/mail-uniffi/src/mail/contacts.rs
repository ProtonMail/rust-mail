use proton_core_common::models::Contact as RealContact;
use std::sync::Arc;

use crate::errors::user_actions::UserActionError;
use crate::{core::datatypes::GroupedContacts, uniffi_async};
use proton_mail_common::errors::user_actions::UserActionError as RealUserActionError;

use super::MailUserSession;

/// Returns grouped contacts by the first grapheme of the name.
///
#[allow(clippy::missing_panics_doc)]
#[proton_uniffi_macros::export_result]
pub async fn contact_list(
    session: Arc<MailUserSession>,
) -> Result<Vec<GroupedContacts>, UserActionError> {
    uniffi_async(async move {
        Result::<_, RealUserActionError>::Ok(
            RealContact::contact_list(session.user_stash())
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    })
    .await
    .map_err(Into::into)
}
