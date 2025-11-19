use crate::{MailContextError, MailContextResult, MailUserContext};
use proton_core_common::datatypes::AddressStatus;
use proton_core_common::models::Address;
use stash::orm::Model;
use stash::params;
use std::sync::Weak;
use tracing::{debug, error};

#[cfg(test)]
#[path = "tests/account_service.rs"]
mod tests;

pub struct AccountService {
    weak: Weak<MailUserContext>,
}

impl AccountService {
    pub fn new(weak: Weak<MailUserContext>) -> Self {
        Self { weak }
    }

    /// Find the first valid non-BYOE sender address.
    #[tracing::instrument(skip_all)]
    pub async fn find_valid_sender_address(&self) -> MailContextResult<Option<Address>> {
        let Some(ctx) = self.weak.upgrade() else {
            error!("Could not upgrade weak ctx reference in AccountService");
            return Err(MailContextError::Other(anyhow::anyhow!(
                "Context reference is no longer valid"
            )));
        };

        let tether = ctx.user_stash().connection().await?;

        let addresses = Address::find(
            "WHERE send=1 AND receive=1 AND status=? ORDER BY display_order".to_owned(),
            params![AddressStatus::Enabled],
            &tether,
        )
        .await
        .map_err(|e| {
            error!("Failed to load addresses for sender validation: {e:?}");
            MailContextError::from(e)
        })?;

        if addresses.is_empty() {
            debug!("No send-enabled addresses found");
            return Ok(None);
        }
        Ok(addresses
            .iter()
            .find(|address| !address.flags.is_some_and(|flags| flags.is_byoe()))
            .cloned())
    }
}
