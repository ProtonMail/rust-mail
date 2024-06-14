use crate::utils::{self, bool_from_integer, bool_to_integer};
use proton_crypto_account::keys::AddressKeys as RealAddressKeys;
#[cfg(feature = "sql")]
use proton_sqlite3::rusqlite;
use serde;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_aux::field_attributes::deserialize_default_from_null;
use serde_repr::{Deserialize_repr, Serialize_repr};
use stash::macros::Model;
use stash::stash::Stash;
use stash::utils::sql_using_serde;
use std::ops::Deref;

utils::string_id!(AddressId);

#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[TableName("addresses")]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    #[IdField]
    #[serde(rename = "ID")]
    pub id: Option<AddressId>,
    #[DbField]
    pub email: String,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub send: bool,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub receive: bool,
    #[DbField]
    pub status: AddressStatus,
    #[DbField]
    #[serde(rename = "DomainID")]
    pub domain_id: Option<String>,
    #[DbField]
    #[serde(rename = "Type")]
    pub address_type: AddressType,
    #[DbField]
    pub display_order: u32,
    #[DbField]
    pub display_name: String,
    #[DbField]
    pub signature: String,
    #[DbField]
    pub keys: AddressKeys,
    #[DbField]
    pub catch_all: bool,
    #[DbField]
    #[serde(rename = "ProtonMX")]
    pub proton_mx: bool,
    #[DbField]
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub signed_key_list: AddressSignedKeyList,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone, Default)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct AddressSignedKeyList {
    #[serde(rename = "MinEpochID")]
    pub min_epoch_id: Option<u64>,
    #[serde(rename = "MaxEpochID")]
    pub max_epoch_id: Option<u64>,
    #[serde(rename = "ExpectedMinEpochID")]
    pub expected_min_epoch_id: Option<u64>,
    pub data: Option<String>,
    pub obsolescence_token: Option<String>,
    pub signature: Option<String>,
    pub revision: u64,
}

sql_using_serde!(AddressSignedKeyList);

#[derive(Debug, Deserialize_repr, Serialize_repr, Eq, PartialEq, Copy, Clone, Hash)]
#[repr(u8)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AddressStatus {
    Disabled = 0,
    Enabled = 1,
    Deleting = 2,
}

#[cfg(feature = "sql")]
impl rusqlite::types::FromSql for AddressStatus {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(AddressStatus::Disabled),
            1 => Ok(AddressStatus::Enabled),
            2 => Ok(AddressStatus::Deleting),
            v => Err(rusqlite::types::FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

#[cfg(feature = "sql")]
impl rusqlite::types::ToSql for AddressStatus {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Owned(
            rusqlite::types::Value::Integer(*self as i64),
        ))
    }
}

#[derive(Debug, Deserialize_repr, Serialize_repr, Eq, PartialEq, Copy, Clone, Hash)]
#[repr(u8)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AddressType {
    Original = 1,
    Alias = 2,
    Custom = 3,
    Premium = 4,
    External = 5,
}

#[cfg(feature = "sql")]
impl rusqlite::types::FromSql for AddressType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(AddressType::Original),
            2 => Ok(AddressType::Alias),
            3 => Ok(AddressType::Custom),
            4 => Ok(AddressType::Premium),
            5 => Ok(AddressType::External),
            v => Err(rusqlite::types::FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

#[cfg(feature = "sql")]
impl rusqlite::types::ToSql for AddressType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Owned(
            rusqlite::types::Value::Integer(*self as i64),
        ))
    }
}

/// Wrapper type around `RealAddressKeys` to implement `FromSql` and `ToSql`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressKeys(pub RealAddressKeys);

impl Deref for AddressKeys {
    type Target = RealAddressKeys;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AddressKeys {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let real_user_keys = RealAddressKeys::deserialize(deserializer)?;
        Ok(AddressKeys(real_user_keys))
    }
}

impl Serialize for AddressKeys {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

sql_using_serde!(AddressKeys);
