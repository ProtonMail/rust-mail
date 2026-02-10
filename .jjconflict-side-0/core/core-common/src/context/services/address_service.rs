use crate::datatypes::AddressStatus;
use crate::models::Address;
use crate::{CoreContextError, CoreContextResult, UserContext};
use stash::orm::Model;
use stash::params;
use std::sync::Weak;

#[cfg(test)]
#[path = "../../tests/address_service.rs"]
mod tests;

pub struct AddressService {
    weak: Weak<UserContext>,
}

impl AddressService {
    #[must_use]
    pub fn new(weak: Weak<UserContext>) -> Self {
        Self { weak }
    }

    /// Find the first valid sender address.
    #[tracing::instrument(skip_all)]
    pub async fn find_valid_sender_address(&self) -> CoreContextResult<Option<Address>> {
        let Some(ctx) = self.weak.upgrade() else {
            return Err(CoreContextError::Other(anyhow::anyhow!(
                "Context reference is no longer valid"
            )));
        };

        let tether = ctx.stash().connection().await?;

        let address = Address::find_first(
            "WHERE send=1 AND receive=1 AND status=? ORDER BY display_order".to_owned(),
            params![AddressStatus::Enabled],
            &tether,
        )
        .await?;

        Ok(address)
    }
}
