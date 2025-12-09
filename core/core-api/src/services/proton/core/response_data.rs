//! Response child data structures for the Proton Core API.
//!
//! This module provides child data types that are used by the response
//! structures when receiving requests from the Proton API.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint response
//! definitions, and NOT have any business logic or other functionality.
//!
//! To be clear, they should only contain data, and not methods; should not be
//! saved in the database; and should not be used for anything except providing
//! an interface for incoming data.
//!
//! Structs in this module should only implement [`Deserialize`], and should not
//! implement [`Serialize`](serde::Serialize). If anything in this module
//! implements [`Serialize`](serde::Serialize), it is a sign that a mistake has
//! been made. The exception here is for testing purposes, e.g. when mocking
//! response data — in which case implementing [`Serialize`](serde::Serialize)
//! conditionally, only in test mode, is advised.
//!
//! Any types that used by both requests and responses should be defined in the
//! [`common`](crate::services::proton::common) module.
//!

use crate::services::proton::prelude::*;
use proton_crypto_account::contacts::ContactCardType;
use proton_crypto_account::keys::{AddressKeys, UserKeys};
use serde::Deserialize;
#[cfg(feature = "mocks")]
use serde::Serialize;
use serde_aux::field_attributes::deserialize_default_from_null;
use serde_json::Error as JsonError;
use serde_json::Value as JsonValue;
use serde_repr::Deserialize_repr;
#[cfg(feature = "mocks")]
use serde_repr::Serialize_repr;
use serde_with::{BoolFromInt, DefaultOnNull, FromInto, serde_as};

mod legacy_feature_flags;
mod unleash_toggles;

pub use legacy_feature_flags::*;
pub use unleash_toggles::*;

//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum Action {
    /// TODO: Document this field.
    Delete = 0,

    /// TODO: Document this field.
    Create = 1,

    /// TODO: Document this field.
    Update = 2,

    /// TODO: Document this field.
    UpdateFlags = 3,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum AddressStatus {
    /// TODO: Document this field.
    Disabled = 0,

    /// TODO: Document this field.
    Enabled = 1,

    /// TODO: Document this field.
    Deleting = 2,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum AddressType {
    /// TODO: Document this variant.
    Original = 1,

    /// TODO: Document this variant.
    Alias = 2,

    /// TODO: Document this variant.
    Custom = 3,

    /// TODO: Document this variant.
    Premium = 4,

    /// TODO: Document this variant.
    External = 5,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum ContactSendingPreferences {
    /// TODO: Document this variant.
    Custom = 0,

    /// TODO: Document this variant.
    Default = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum DateFormat {
    /// TODO: Document this variant.
    Default = 0,

    /// TODO: Document this variant.
    DdMmYyyy = 1,

    /// TODO: Document this variant.
    MmDdYyyy = 2,

    /// TODO: Document this variant.
    YyyyMmDd = 3,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum Density {
    /// TODO: Document this variant.
    Comfortable = 0,

    /// TODO: Document this variant.
    Compact = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum EarlyAccess {
    /// TODO: Document this variant.
    Regular = 0,

    /// TODO: Document this variant.
    Beta = 1,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum LogAuth {
    /// TODO: Document this variant.
    Disabled = 0,

    /// TODO: Document this variant.
    Basic = 1,

    /// TODO: Document this variant.
    Advanced = 2,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum PasswordMode {
    /// TODO: Document this variant.
    One = 1,

    /// TODO: Document this variant.
    Two = 2,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum TfaStatus {
    /// TODO: Document this variant.
    None = 0,

    /// TODO: Document this variant.
    Totp = 1,

    /// TODO: Document this variant.
    Fido2 = 2,

    /// TODO: Document this variant.
    TotpOrFido2 = 3,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum TimeFormat {
    /// TODO: Document this variant.
    Default = 0,

    /// TODO: Document this variant.
    H24 = 1,

    /// TODO: Document this variant.
    H12 = 2,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum UserMnemonicStatus {
    /// TODO: Document this variant.
    Disabled = 0,

    /// TODO: Document this variant.
    EnabledButNotSet = 1,

    /// TODO: Document this variant.
    EnabledNeedsReactivation = 2,

    /// TODO: Document this variant.
    EnabledAndSet = 3,

    /// TODO: Document this variant.
    Unknown = 4,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[repr(u8)]
pub enum UserType {
    /// TODO: Document this variant.
    Proton = 1,

    /// TODO: Document this variant.
    Managed = 2,

    /// TODO: Document this variant.
    External = 3,

    /// Credentialless user
    CredentialLess = 4,

    Unknown(u8),
}

impl From<u8> for UserType {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Proton,
            2 => Self::Managed,
            3 => Self::External,
            4 => Self::CredentialLess,
            v => Self::Unknown(v),
        }
    }
}

impl From<UserType> for u8 {
    fn from(value: UserType) -> Self {
        match value {
            UserType::Proton => 1,
            UserType::Managed => 2,
            UserType::External => 3,
            UserType::CredentialLess => 4,
            UserType::Unknown(v) => v,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[repr(u32)]
pub enum Role {
    None = 0,
    Member = 1,
    Admin = 2,
    Unknown(u32),
}

impl From<u32> for Role {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Member,
            2 => Self::Admin,
            v => Self::Unknown(v),
        }
    }
}

impl From<Role> for u32 {
    fn from(value: Role) -> Self {
        match value {
            Role::None => 0,
            Role::Member => 1,
            Role::Admin => 2,
            Role::Unknown(v) => v,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum WeekStart {
    /// TODO: Document this variant.
    Default = 0,

    /// TODO: Document this variant.
    Monday = 1,

    /// TODO: Document this variant.
    Saturday = 6,

    /// TODO: Document this variant.
    Sunday = 7,
}

//  STRUCTS
//==============================================================================

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: AddressId,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub address_type: AddressType,

    /// TODO: Document this field.
    pub catch_all: bool,

    /// TODO: Document this field.
    pub display_name: String,

    /// TODO: Document this field.
    #[serde(rename = "DomainID")]
    pub domain_id: Option<String>,

    /// TODO: Document this field.
    pub email: String,

    /// TODO: Document this field.
    pub keys: AddressKeys,

    /// TODO: Document this field.
    pub order: u32,

    /// TODO: Document this field.
    #[serde(rename = "ProtonMX")]
    pub proton_mx: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub receive: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub send: bool,

    /// TODO: Document this field.
    pub signature: String,

    /// TODO: Document this field.
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub signed_key_list: AddressSignedKeyList,

    /// TODO: Document this field.
    pub status: AddressStatus,
    pub flags: AddressFlags,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[repr(transparent)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
pub struct AddressFlags(pub u32);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct AddressSignedKeyList {
    /// TODO: Document this field.
    pub data: Option<String>,

    /// TODO: Document this field.
    #[serde(rename = "ExpectedMinEpochID")]
    pub expected_min_epoch_id: Option<u64>,

    /// TODO: Document this field.
    #[serde(rename = "MaxEpochID")]
    pub max_epoch_id: Option<u64>,

    /// TODO: Document this field.
    #[serde(rename = "MinEpochID")]
    pub min_epoch_id: Option<u64>,

    /// TODO: Document this field.
    pub obsolescence_token: Option<String>,

    /// TODO: Document this field.
    pub revision: u64,

    /// TODO: Document this field.
    pub signature: Option<String>,
}

/// Data for an event related to an [`AddressEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct AddressEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: AddressId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub address: Option<Address>,
}

/// Represents partial contact information returned by the API.
///
/// The partial contact information does not contain the contact emails and the
/// v-cards.
///
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ContactBasic {
    #[serde(rename = "ID")]
    pub id: ContactId,

    /// TODO: Document this field.
    pub create_time: u64,

    /// TODO: Document this field.
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<LabelId>,

    /// TODO: Document this field.
    pub modify_time: u64,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    #[serde(rename = "UID")]
    pub uid: ContactUID,
}

/// Represents a contact card returned by the API.
///
/// Contact cards contain information encoded as a v-card. Cards can be
/// encrypted or signed with the user keys.
///
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ContactCard {
    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub card_type: ContactCardType,

    /// TODO: Document this field.
    pub data: String,

    /// TODO: Document this field.
    pub signature: Option<String>,
}

/// Models the contact email addresses for a contact returned by the API.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ContactEmail {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: ContactEmailId,

    /// TODO: Document this field.
    #[serde(rename = "ContactID")]
    pub contact_id: ContactId,

    /// TODO: Document this field.
    pub canonical_email: PrivateEmail,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub contact_type: Vec<String>,

    /// TODO: Document this field.
    pub defaults: ContactSendingPreferences,

    /// TODO: Document this field.
    pub email: PrivateEmail,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub is_proton: bool,

    /// TODO: Document this field.
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<LabelId>,

    /// TODO: Document this field.
    pub last_used_time: u64,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub order: u32,
}

/// Data for an event related to a [`ContactEmail`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ContactEmailEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: ContactEmailId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub contact_email: Option<ContactEmail>,
}

/// Data for an event related to a [`ContactBasic`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ContactEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: ContactId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub contact: Option<ContactFull>,
}

/// A complete contact returned by the API.
///
/// Compared to the [`ContactBasic`], it additionally includes all associated
/// contact emails ([`ContactEmail`]) and cards ([`ContactCard`]).
///
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ContactFull {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: ContactId,

    /// TODO: Document this field.
    pub cards: Vec<ContactCard>,

    /// TODO: Document this field.
    pub contact_emails: Vec<ContactEmail>,

    /// TODO: Document this field.
    pub create_time: u64,

    /// TODO: Document this field.
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<LabelId>,

    /// TODO: Document this field.
    pub modify_time: u64,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,

    /// TODO: Document this field.
    #[serde(rename = "UID")]
    pub uid: ContactUID,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Email {
    /// TODO: Document this field.
    pub notify: u8,

    /// TODO: Document this field.
    pub reset: u8,

    /// TODO: Document this field.
    pub status: u8,

    /// TODO: Document this field.
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub value: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Fido2Info {
    /// TODO: Document this field.
    pub authentication_options: JsonValue,

    /// TODO: Document this field.
    pub registered_keys: Option<JsonValue>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct FidoKey {
    /// TODO: Document this field.
    pub attestation_format: String,

    /// TODO: Document this field.
    #[serde(rename = "CredentialID")]
    pub credential_id: Vec<i32>,

    /// TODO: Document this field.
    pub name: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[allow(clippy::struct_excessive_bools)]
pub struct Flags {
    /// TODO: Document this field.
    #[serde(rename = "has-temporary-password")]
    pub has_temporary_password: bool,

    /// Whether the user has a BYOE address.
    #[serde(rename = "has-a-byoe-address")]
    pub has_a_byoe_address: bool,

    /// TODO: Document this field.
    #[serde(rename = "no-login")]
    pub no_login: bool,

    /// TODO: Document this field.
    #[serde(rename = "no-proton-address")]
    pub no_proton_address: bool,

    /// TODO: Document this field.
    #[serde(rename = "onboard-checklist-storage-granted")]
    pub onboard_checklist_storage_granted: bool,

    /// TODO: Document this field.
    pub protected: bool,

    /// TODO: Document this field.
    #[serde(rename = "recovery-attempt")]
    pub recovery_attempt: bool,

    /// TODO: Document this field.
    pub sso: bool,

    /// TODO: Document this field.
    #[serde(rename = "test-account")]
    pub test_account: bool,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct HighSecurity {
    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub eligible: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub value: bool,
}

/// Information for the human verification challenge.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct HumanVerificationChallenge {
    pub description: String,
    pub direct: u8,
    pub expires_at: u64,
    pub human_verification_methods: Vec<String>,
    pub human_verification_token: String,
    pub web_url: String,
}

impl HumanVerificationChallenge {
    pub fn from_value(value: JsonValue) -> Result<Self, JsonError> {
        serde_json::from_value(value)
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Password {
    /// TODO: Document this field.
    pub mode: PasswordMode,

    /// TODO: Document this field.
    pub expiration_time: Option<u64>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Phone {
    /// TODO: Document this field.
    pub notify: u8,

    /// TODO: Document this field.
    pub reset: u8,

    /// TODO: Document this field.
    pub status: u8,

    /// TODO: Document this field.
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub value: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct ProductUsedSpace {
    /// TODO: Document this field.
    pub calendar: i64,

    /// TODO: Document this field.
    pub contact: i64,

    /// TODO: Document this field.
    pub drive: i64,

    /// TODO: Document this field.
    pub mail: i64,

    /// TODO: Document this field.
    pub pass: i64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Referral {
    /// TODO: Document this field.
    pub eligible: bool,

    /// TODO: Document this field.
    pub link: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Salt {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: SaltId,

    /// TODO: Document this field.
    pub key_salt: Option<String>,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct SettingsFlags {
    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub welcomed: bool,
    /// `EasyDeviceMigration` (QR Login) opt out. The user can choose to disable the feature.
    #[serde_as(as = "BoolFromInt")]
    pub edm_opt_out: bool,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct TwoFa {
    /// TODO: Document this field.
    pub allowed: TfaStatus,

    /// TODO: Document this field.
    pub enabled: TfaStatus,

    /// TODO: Document this field.
    pub expiration_time: Option<u64>,

    /// TODO: Document this field.
    #[serde(default)]
    pub registered_keys: Vec<FidoKey>,
}

/// TODO: Document this struct.
/// Represents an API user
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct User {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: UserId,

    /// TODO: Document this field.
    pub create_time: u64,

    /// TODO: Document this field.
    pub credit: i64,

    /// TODO: Document this field.
    pub currency: String,

    /// Indicates the delinquency status of the user's account.
    pub delinquent: DelinquentState,

    /// TODO: Document this field.
    pub display_name: Option<String>,

    /// TODO: Document this field.
    pub email: String,

    /// TODO: Document this field.
    pub flags: Flags,

    /// TODO: Document this field.
    pub keys: UserKeys,

    /// TODO: Document this field.
    pub max_space: i64,

    /// TODO: Document this field.
    pub max_upload: i64,

    /// TODO: Document this field.
    pub mnemonic_status: UserMnemonicStatus,

    /// TODO: Document this field.
    pub name: Option<String>,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub private: bool,

    /// TODO: Document this field.
    pub product_used_space: ProductUsedSpace,

    /// TODO: Document this field.
    #[serde_as(as = "FromInto<u32>")]
    pub role: Role,

    /// TODO: Document this field.
    pub services: u32,

    /// TODO: Document this field.
    pub subscribed: u32,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub to_migrate: bool,

    /// TODO: Document this field.
    pub used_space: i64,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    #[serde_as(as = "FromInto<u8>")]
    pub user_type: UserType,
}

/// Represents the delinquent state of the user.
///
/// This enum indicates the payment status of the user's account.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize_repr, Eq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[serde(rename_all = "PascalCase")]
#[repr(u32)]
pub enum DelinquentState {
    /// The user's account is fully paid.
    Paid = 0,
    /// The user's account is available but not yet paid.
    Available = 1,
    /// The user's account has an overdue payment.
    Overdue = 2,
    /// The user's account is delinquent due to unpaid dues.
    Delinquent = 3,
    /// The user's payment has not been received.
    NotReceived = 4,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct UserSettings {
    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub crash_reports: bool,

    /// TODO: Document this field.
    pub date_format: DateFormat,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub device_recovery: bool,

    /// TODO: Document this field.
    pub density: Density,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub early_access: bool,

    /// TODO: Document this field.
    pub email: Email,

    /// TODO: Document this field.
    pub flags: SettingsFlags,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub hide_side_panel: bool,

    /// TODO: Document this field.
    pub high_security: HighSecurity,

    /// TODO: Document this field.
    pub invoice_text: String,

    /// TODO: Document this field.
    pub locale: String,

    /// TODO: Document this field.
    pub log_auth: LogAuth,

    /// TODO: Document this field.
    pub news: u32,

    /// TODO: Document this field.
    pub password: Password,

    /// TODO: Document this field.
    pub phone: Phone,

    /// TODO: Document this field.
    pub referral: Option<Referral>,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub session_account_recovery: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub telemetry: bool,

    /// TODO: Document this field.
    pub time_format: TimeFormat,

    /// TODO: Document this field.
    #[serde(rename = "2FA")]
    pub two_factor_auth: TwoFa,

    /// TODO: Document this field.
    pub week_start: WeekStart,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub welcome: bool,
}

/// TODO: Document this struct.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Label {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: LabelId,

    /// TODO: Document this field.
    #[serde(rename = "ParentID")]
    pub parent_id: Option<LabelId>,

    /// TODO: Document this field.
    pub color: String,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub display: bool,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub expanded: bool,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub label_type: LabelType,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub notify: bool,

    /// TODO: Document this field.
    #[serde(default)]
    pub order: u32,

    /// TODO: Document this field.
    pub path: Option<String>,

    /// TODO: Document this field.
    #[serde_as(as = "DefaultOnNull<BoolFromInt>")]
    pub sticky: bool,
}

#[cfg(feature = "mocks")]
impl Label {
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            id: LabelId::from(""),
            parent_id: None,
            color: String::default(),
            display: false,
            expanded: false,
            label_type: LabelType::Label,
            name: String::default(),
            notify: false,
            order: 0,
            path: None,
            sticky: false,
        }
    }
}

/// Data for an event related to a [`LabelEvent`] record.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct LabelEvent {
    /// TODO: Document this field.
    #[serde(rename = "ID")]
    pub id: LabelId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub label: Option<Label>,
}

/// Core event data structure that matches the core fields from events.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct CoreEvent {
    #[serde(rename = "EventID")]
    pub event_id: EventId,

    pub addresses: Option<Vec<AddressEvent>>,

    pub labels: Option<Vec<LabelEvent>>,

    pub product_used_space: Option<ProductUsedSpace>,

    pub used_space: Option<i64>,

    pub user: Option<User>,

    pub user_settings: Option<UserSettings>,

    pub contacts: Option<Vec<ContactEvent>>,

    /// Indicates whether to refresh.
    pub refresh: u8,

    /// Whether we need to request more events after this.
    #[serde(rename = "More")]
    #[serde_as(as = "BoolFromInt")]
    pub has_more: bool,
}
