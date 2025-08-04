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

        // HACK while the calendar key passphrase is going to be encrypted
        //      towards `id`'s address keys, Proton-to-Proton invites are still
        //      encrypted towards the recipient's public key.
        //
        // i.e. if you have a calendar encrypted for foo@protonmail.com, but you
        // receive an invite on foo@protom.me, the calendar will be encrypted
        // towards foo@protonmail.com, while the invite will be encrypted
        // towards foo@proton.me.
        //
        // "Merging" both keys here is the easiest way of handling this.
        Ok(UnlockedAddressKeys(
            keys1.0.into_iter().chain(keys2.0).collect(),
        ))
    }
}
