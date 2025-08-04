use crate::{MailContextError, MailUserContext};
use proton_calendar_common as cal;
use proton_core_api::services::proton::AddressId;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use stash::stash::Tether;
use tracing::error;

pub struct RsvpKeys<'a> {
    ctx: &'a MailUserContext,
    tether: &'a Tether,
    address_id: &'a AddressId,
}

impl<'a> RsvpKeys<'a> {
    pub fn new(ctx: &'a MailUserContext, tether: &'a Tether, address_id: &'a AddressId) -> Self {
        Self {
            ctx,
            tether,
            address_id,
        }
    }
}

impl cal::RsvpKeys for RsvpKeys<'_> {
    type Error = MailContextError;

    async fn get_address_keys<P>(
        &self,
        pgp: &P,
        id: &AddressId,
    ) -> Result<UnlockedAddressKeys<P>, Self::Error>
    where
        P: PGPProviderSync,
    {
        let keys1 = self
            .ctx
            .unlocked_address_keys(pgp, self.tether, id)
            .await
            .inspect_err(|err| error!("Couldn't unlock address keys: {err:?}"))?;

        let keys2 = self
            .ctx
            .unlocked_address_keys(pgp, self.tether, self.address_id)
            .await
            .inspect_err(|err| error!("Couldn't unlock address keys: {err:?}"))?;

        Ok(UnlockedAddressKeys(
            keys1.0.into_iter().chain(keys2.0).collect(),
        ))
    }
}
