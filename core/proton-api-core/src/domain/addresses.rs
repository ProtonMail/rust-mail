use proton_crypto_account::domain::AddressKeys;
use proton_sqlite3::rusqlite;
use serde;
use serde::{Deserialize, Serialize};
use serde_aux::field_attributes::deserialize_default_from_null;
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::utils::{self, bool_from_integer, bool_to_integer};

utils::string_id!(AddressId);

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    #[serde(rename = "ID")]
    pub id: AddressId,
    pub email: String,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub send: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub receive: bool,
    pub status: AddressStatus,
    #[serde(rename = "DomainID")]
    pub domain_id: Option<String>,
    #[serde(rename = "Type")]
    pub address_type: AddressType,
    pub order: u32,
    pub display_name: String,
    pub signature: String,
    pub keys: AddressKeys,
    pub catch_all: bool,
    #[serde(rename = "ProtonMX")]
    pub proton_mx: bool,
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub signed_key_list: AddressSignedKeyList,
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
