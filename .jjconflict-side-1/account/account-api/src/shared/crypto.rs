use proton_crypto_account::errors::{AccountCryptoError, SKLError};
use proton_crypto_account::keys::{
    KeyFlag, KeyId, LocalAddressKey, LocalSignedKeyList, LocalUserKey, UnlockedAddressKeys,
    UnlockedUserKey,
};
use proton_crypto_account::proton_crypto::CryptoError;
use proton_crypto_account::proton_crypto::crypto::{KeyGeneratorAlgorithm, PGPProviderSync};
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::{KeySalt, KeySecret, SaltError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SharedCryptoError {
    #[error("Salt error: {0}")]
    Salt(#[from] SaltError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Account crypto error: {0}")]
    AccountCrypto(#[from] AccountCryptoError),

    #[error("SKL error: {0}")]
    SKL(#[from] SKLError),
}

pub struct NewUserKey {
    pub key: LocalUserKey,
    pub salt: KeySalt,
    pub pass: KeySecret,
}

impl NewUserKey {
    pub fn init(
        srp: &impl SRPProvider,
        pgp: &impl PGPProviderSync,
        pass: &str,
    ) -> Result<Self, SharedCryptoError> {
        let algo = KeyGeneratorAlgorithm::default();
        let salt = KeySalt::generate();
        let pass = salt.salted_key_passphrase(srp, pass.as_bytes())?;
        let key = LocalUserKey::generate(pgp, algo, &pass)?;

        Ok(Self { key, salt, pass })
    }

    pub fn init_addr<P: PGPProviderSync>(
        &self,
        pgp: &P,
        addr: &str,
    ) -> Result<NewAddrKey, SharedCryptoError> {
        let key_id = new_key_id();
        let user_key = self.key.unlock_and_assign_key_id(pgp, key_id, &self.pass)?;

        NewAddrKey::init(pgp, &user_key, addr)
    }
}

pub struct NewAddrKey {
    pub key: LocalAddressKey,
    pub skl: LocalSignedKeyList,
}

impl NewAddrKey {
    pub fn init<P: PGPProviderSync>(
        pgp: &P,
        user_key: &UnlockedUserKey<P>,
        addr: &str,
    ) -> Result<Self, SharedCryptoError> {
        let algo = KeyGeneratorAlgorithm::default();
        let key = create_addr_key(pgp, algo, user_key, addr)?;
        let skl = create_addr_skl(pgp, user_key, &key)?;

        Ok(Self { key, skl })
    }
}

fn create_addr_key<P: PGPProviderSync>(
    pgp: &P,
    alg: KeyGeneratorAlgorithm,
    user_key: &UnlockedUserKey<P>,
    addr: &str,
) -> Result<LocalAddressKey, SharedCryptoError> {
    let addr_key = LocalAddressKey::generate(pgp, addr, alg, KeyFlag::default(), true, user_key)?;

    Ok(addr_key)
}

fn create_addr_skl<P: PGPProviderSync>(
    pgp: &P,
    user_key: &UnlockedUserKey<P>,
    addr_key: &LocalAddressKey,
) -> Result<LocalSignedKeyList, SharedCryptoError> {
    let key_id = new_key_id();
    let addr_key = addr_key.unlock_and_assign_key_id(pgp, key_id.clone(), user_key)?;
    let addr_skl = LocalSignedKeyList::generate(pgp, &UnlockedAddressKeys(vec![addr_key]))?;

    Ok(addr_skl)
}

/// Generates a dummy key ID.
///
/// This is a bit annoying in the current crypto APIs, you have to pass a dummy `KeyID` to use them.
/// In theory we could introduce another model, but I think it would be an overkill.
/// For the sign-up operations a key with a dummy key id is fine.
fn new_key_id() -> KeyId {
    KeyId(String::default())
}
