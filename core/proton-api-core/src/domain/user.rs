use proton_crypto_account::domain::{KeyError, PrivateKeyRing, UserKeys};
use proton_crypto_account::keyring::LockedKey;
use proton_crypto_account::proton_crypto::crypto::{PGPProvider, PGPProviderSync};
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::{SaltError, SaltedPassword, Salts};
use serde;
use serde::{Deserialize, Serialize};

crate::utils::string_id!(UserUid);
impl secrecy::Zeroize for UserUid {
    fn zeroize(&mut self) {
        self.0.zeroize()
    }
}

impl secrecy::CloneableSecret for UserUid {}

impl secrecy::DebugSecret for UserUid {}

crate::utils::string_id!(UserId);

/// Represents an API user
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct User {
    #[serde(rename = "ID")]
    pub id: UserId,
    pub name: String,
    pub display_name: String,
    pub email: String,
    pub used_space: i64,
    pub max_space: i64,
    pub max_upload: i64,
    pub credit: i64,
    pub currency: String,
    pub keys: UserKeys,
}

#[derive(Debug, thiserror::Error)]
pub enum UserSaltError {
    #[error("Could not find primary key")]
    PrimaryKeyNotFound,
    #[error("{0}")]
    Key(
        #[source]
        #[from]
        KeyError,
    ),
    #[error("{0}")]
    Salt(
        #[source]
        #[from]
        SaltError,
    ),
}

impl User {
    pub fn get_primary_key(&self) -> Option<&LockedKey> {
        self.keys.0.iter().find(|&k| k.primary)
    }

    pub fn salt_password<SRP: SRPProvider>(
        &self,
        provider: &SRP,
        salts: &Salts,
        mailbox_password: impl AsRef<[u8]>,
    ) -> Result<SaltedPassword<<SRP as SRPProvider>::HashedPassword>, UserSaltError> {
        let Some(primary_key) = self.get_primary_key() else {
            return Err(UserSaltError::PrimaryKeyNotFound);
        };
        let salted = salts.salt_for_key(provider, &primary_key.id, mailbox_password.as_ref())?;
        Ok(salted)
    }

    pub fn unlock_keys<SRP: SRPProvider, PGP: PGPProviderSync>(
        &self,
        provider: &PGP,
        salted_password: &SaltedPassword<<SRP as SRPProvider>::HashedPassword>,
    ) -> Result<PrivateKeyRing<<PGP as PGPProvider>::PrivateKey>, KeyError> {
        self.keys.unlock(provider, salted_password)
    }
}
