use proton_api_core::domain::ProtonBoolean;
use proton_api_core::exports::serde;
use proton_api_core::exports::serde_repr::{Deserialize_repr, Serialize_repr};
use proton_crypto_rs::domain::AddressKeys;
use serde::{Deserialize, Serialize};

proton_api_core::utils::string_id!(AddressId);

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct Address {
    #[serde(rename = "ID")]
    pub id: AddressId,
    pub email: String,
    pub send: ProtonBoolean,
    pub receive: ProtonBoolean,
    pub status: AddressStatus,
    #[serde(rename = "Type")]
    pub address_type: AddressType,
    pub order: u32,
    pub display_name: String,
    pub keys: AddressKeys,
}

#[derive(Debug, Deserialize_repr, Serialize_repr, Eq, PartialEq, Copy, Clone, Hash)]
#[repr(u8)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AddressStatus {
    Disabled = 0,
    Enabled = 1,
    Deleting = 2,
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
