use crate::domain::ProtonBoolean;
use proton_crypto_account::domain::{KeyError, PrivateKeyRing, UserKeys};
use proton_crypto_account::keyring::LockedKey;
use proton_crypto_account::proton_crypto::crypto::{PGPProvider, PGPProviderSync};
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::{SaltError, SaltedPassword, Salts};
use serde;
use serde::{Deserialize, Serialize};

crate::utils::string_id!(Uid);
impl secrecy::Zeroize for Uid {
    fn zeroize(&mut self) {
        self.0.zeroize()
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
pub struct UserProductUsedSpace {
    pub calendar: i64,
    pub contact: i64,
    pub drive: i64,
    pub mail: i64,
    pub pass: i64,
}
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct UserFlags {
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
    pub product_used_space: UserProductUsedSpace,
    pub to_migrate: ProtonBoolean,
    pub mnemonic_status: UserMnemonicStatus,
    pub role: u32,
    pub private: u32,
    pub subscribed: u32,
    pub services: u32,
    pub delinquent: u32,
    pub flags: UserFlags,
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

    pub fn user_name(&self) -> &str {
        if let Some(display_name) = self.display_name.as_deref() {
            display_name
        } else if let Some(name) = self.name.as_deref() {
            name
        } else {
            &self.email
        }
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
