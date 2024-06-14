#![allow(clippy::module_name_repetitions)]

use crate::utils::{bool_from_integer, bool_to_integer};
use proton_crypto_account::keys::{DecryptedUserKey, UnlockResult, UserKeys as RealUserKeys};
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::{KeySecret, SaltError as CryptoSaltError, Salts};
use serde;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stash::macros::Model;
use stash::stash::Stash;
use stash::utils::sql_using_serde;
use std::ops::Deref;

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

sql_using_serde!(ProductUsedSpace);

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

sql_using_serde!(Flags);

/// Represents an API user
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[TableName("users")]
pub struct User {
    #[IdField]
    #[serde(rename = "ID")]
    pub id: Option<UserId>,
    #[DbField]
    pub name: Option<String>,
    #[DbField]
    pub display_name: Option<String>,
    #[DbField]
    pub email: String,
    #[DbField]
    pub used_space: i64,
    #[DbField]
    pub max_space: i64,
    #[DbField]
    pub max_upload: i64,
    #[DbField]
    #[serde(rename = "Type")]
    pub user_type: UserType,
    #[DbField]
    pub create_time: u64,
    #[DbField]
    pub credit: i64,
    #[DbField]
    pub currency: String,
    #[DbField]
    pub keys: UserKeys,
    #[DbField]
    pub product_used_space: ProductUsedSpace,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    #[DbField]
    pub to_migrate: bool,
    #[DbField]
    pub mnemonic_status: UserMnemonicStatus,
    #[DbField]
    pub role: u32,
    #[DbField]
    pub private: u32,
    #[DbField]
    pub subscribed: u32,
    #[DbField]
    pub services: u32,
    #[DbField]
    pub delinquent: u32,
    #[DbField]
    pub flags: Flags,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

/// Wrapper type around `RealUserKeys` to implement `FromSql` and `ToSql`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserKeys(pub RealUserKeys);

impl Deref for UserKeys {
    type Target = RealUserKeys;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for UserKeys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let real_user_keys = RealUserKeys::deserialize(deserializer)?;
        Ok(UserKeys(real_user_keys))
    }
}

impl Serialize for UserKeys {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

sql_using_serde!(UserKeys);

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
    pub fn get_primary_key(&self) -> Option<&proton_crypto_account::keys::LockedKey> {
        self.keys.0 .0.iter().find(|&k| k.primary)
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
    ) -> Result<KeySecret, SaltError> {
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
    pub fn unlock_keys<PGP: PGPProviderSync>(
        &self,
        provider: &PGP,
        salted_password: &KeySecret,
    ) -> UnlockResult<DecryptedUserKey<<PGP>::PrivateKey, <PGP>::PublicKey>> {
        self.keys.unlock(provider, salted_password)
    }
}
