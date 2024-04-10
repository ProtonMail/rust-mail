use crate::utils::{bool_from_integer, bool_to_integer};
use proton_crypto_account::domain::{DecryptedUserKey, UnlockResult, UserKeys};
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::{SaltError as CryptoSaltError, SaltedPassword, Salts};
use serde;
use serde::{Deserialize, Serialize};

crate::utils::string_id!(Uid);
impl secrecy::Zeroize for Uid {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl secrecy::CloneableSecret for Uid {}

impl secrecy::DebugSecret for Uid {}

crate::utils::string_id!(UserId);

new_integer_enum!(u8,UserType {
    Proton = 1,
    Managed = 2,
    External = 3,
});

new_integer_enum!(u8, UserMnemonicStatus {
    Disabled = 0,
    EnabledButNotSet = 1,
    EnabledNeedsReActivation = 2,
    EnabledAndSet = 3,
    Unknown = 4,
});

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProductUsedSpace {
    pub calendar: i64,
    pub contact: i64,
    pub drive: i64,
    pub mail: i64,
    pub pass: i64,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Flags {
    pub protected: bool,
    #[serde(rename = "onboard-checklist-storage-granted")]
    pub onboard_checklist_storage_granted: bool,
    #[serde(rename = "has-temporary-password")]
    pub has_temporary_password: bool,
    #[serde(rename = "test-account")]
    pub test_account: bool,
    #[serde(rename = "no-login")]
    pub no_login: bool,
    #[serde(rename = "recovery-attempt")]
    pub recovery_attempt: bool,
    pub sso: bool,
    #[serde(rename = "no-proton-address")]
    pub no_proton_address: bool,
}

/// Represents an API user
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct User {
    #[serde(rename = "ID")]
    pub id: UserId,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub email: String,
    pub used_space: i64,
    pub max_space: i64,
    pub max_upload: i64,
    #[serde(rename = "Type")]
    pub user_type: UserType,
    pub create_time: u64,
    pub credit: i64,
    pub currency: String,
    pub keys: UserKeys,
    pub product_used_space: ProductUsedSpace,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub to_migrate: bool,
    pub mnemonic_status: UserMnemonicStatus,
    pub role: u32,
    pub private: u32,
    pub subscribed: u32,
    pub services: u32,
    pub delinquent: u32,
    pub flags: Flags,
}

#[derive(Debug, thiserror::Error)]
pub enum SaltError {
    #[error("Could not find primary key")]
    PrimaryKeyNotFound,
    #[error("{0}")]
    Key(
        #[source]
        #[from]
        proton_crypto_account::errors::KeyError,
    ),
    #[error("{0}")]
    Salt(
        #[source]
        #[from]
        CryptoSaltError,
    ),
}

impl User {
    /// Get the users primary key.
    #[must_use]
    pub fn get_primary_key(&self) -> Option<&proton_crypto_account::domain::LockedKey> {
        self.keys.0.iter().find(|&k| k.primary)
    }

    /// Get the user's display name.
    #[must_use]
    pub fn user_name(&self) -> &str {
        if let Some(display_name) = self.display_name.as_deref() {
            display_name
        } else if let Some(name) = self.name.as_deref() {
            name
        } else {
            &self.email
        }
    }

    /// Salt a user password.
    ///
    /// # Errors
    /// Returns error if the password can't be salted.
    pub fn salt_password<SRP: SRPProvider>(
        &self,
        provider: &SRP,
        salts: &Salts,
        mailbox_password: impl AsRef<[u8]>,
    ) -> Result<SaltedPassword<<SRP as SRPProvider>::HashedPassword>, SaltError> {
        let Some(primary_key) = self.get_primary_key() else {
            return Err(SaltError::PrimaryKeyNotFound);
        };
        let salted = salts.salt_for_key(provider, &primary_key.id, mailbox_password.as_ref())?;
        Ok(salted)
    }

    /// Unlock the user's encryption keys.
    ///
    /// # Errors
    /// Returns error if the keys can't be unlocked.
    pub fn unlock_keys<SRP: SRPProvider, PGP: PGPProviderSync>(
        &self,
        provider: &PGP,
        salted_password: &SaltedPassword<<SRP as SRPProvider>::HashedPassword>,
    ) -> UnlockResult<DecryptedUserKey<<PGP>::PrivateKey, <PGP>::PublicKey>> {
        self.keys.unlock(provider, salted_password)
    }
}
