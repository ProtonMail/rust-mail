use std::sync::Weak;

use proton_core_common::models::Contact;
use tracing::instrument;

use crate::{MailContextError, MailContextResult, MailUserContext};

#[instrument(skip_all)]
pub async fn run(ctx: &Weak<MailUserContext>) -> MailContextResult<()> {
    let ctx = ctx.upgrade().ok_or(MailContextError::LostContext)?;

    let mut tether = ctx.user_stash().connection().await?;

    let contacts_without_emails = Contact::without_emails(&tether).await?;
    let session = ctx.session();

    tracing::info!(
        "Found {} contacts without emails",
        contacts_without_emails.len()
    );

    for id in contacts_without_emails {
        Contact::force_sync_with_card(id, session, &mut tether).await?;
    }

    Ok(())
}
