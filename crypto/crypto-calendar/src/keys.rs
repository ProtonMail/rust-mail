use proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::keys::{UnlockedAddressKeys, UnlockedUserKeys};

pub struct UnlockedKeys<P>
where
    P: PGPProviderSync,
{
    pub user_keys: UnlockedUserKeys<P>,
    pub address_keys: UnlockedAddressKeys<P>,
}
