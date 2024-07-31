//! Persistent data types for the Proton Core common library.
//!
//! This module contains various data types used by the Proton Core common
//! library. Many of these are used by the models in the [`models`](crate::models)
//! module, where they represent child data structures for the models' fields.
//! The models themselves should not be placed in this module.
//!
//! All data types used by [`Model`](stash::macros::Model) fields need to be
//! convertible to and from database-compatible format using [`ToSql`] and
//! [`FromSql`]. They do not generally need to be serializable or
//! deserializable, as they are not used for network communication or any other
//! interchange purpose as a general requirement, and so implementation of
//! [`Serialize`] and [`Deserialize`] is not necessary and may be a sign of a
//! mistake. The exception here is when these [`serde`] conversions are
//! desirable to lean on in order to provide conversion to and from SQL types,
//! for instance using [`sql_using_serde`], as a convenience mechanism. This is
//! notably useful when wanting to store types as JSON in a database field, for
//! instance.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need. Such conversions are usually very simple and indeed in many
//! cases can be done without altering any data in memory.
//!
//! This separation does cause some duplication, but the overlap is not total.
//! The various implementations for the types differ in each place; any logic
//! for the application is in the application types and not the API types; and
//! the distinction allows customisation of how the application deals with and
//! stores its related data. Additionally, it promotes wider usability, as each
//! application that depends upon the API types can interpret and managed them
//! in its own way.
//!
//! Note: The current exception to this organisation rule is that of the data
//! types used for events. These are not saved in the database, and so do not
//! have a related model, and their data types are not placed into this module
//! as they are not related to modelling of persistent data against storage.
//! Hence event data types are placed into the [`events`](crate::events) module.
//!

use core::fmt;
use proton_api_core::services::proton::common::{
    LightOrDarkMode as ApiLightOrDarkMode, RemoteId as ApiRemoteId,
};
use proton_api_core::services::proton::response_data::{
    AddressSignedKeyList as ApiAddressSignedKeyList, AddressStatus as ApiAddressStatus,
    AddressType as ApiAddressType, CardType as ApiCardType,
    ContactSendingPreferences as ApiContactSendingPreferences, DateFormat as ApiDateFormat,
    Density as ApiDensity, EarlyAccess as ApiEarlyAccess, Email as ApiEmail, FidoKey as ApiFidoKey,
    Flags as ApiFlags, HighSecurity as ApiHighSecurity, LogAuth as ApiLogAuth,
    Password as ApiPassword, Phone as ApiPhone, ProductUsedSpace as ApiProductUsedSpace,
    Referral as ApiReferral, SettingsFlags as ApiSettingsFlags, TfaStatus as ApiTfaStatus,
    TimeFormat as ApiTimeFormat, TwoFa as ApiTwoFa, UserMnemonicStatus as ApiUserMnemonicStatus,
    UserType as ApiUserType, WeekStart as ApiWeekStart,
};
use proton_crypto_account::keys::{AddressKeys as RealAddressKeys, UserKeys as RealUserKeys};
use secrecy::{CloneableSecret, DebugSecret};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::utils::sql_using_serde;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use zeroize::Zeroize;

//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum AddressStatus {
    /// TODO: Document this field.
    Disabled = 0,

    /// TODO: Document this field.
    Enabled = 1,

    /// TODO: Document this field.
    Deleting = 2,
}

impl From<ApiAddressStatus> for AddressStatus {
    fn from(value: ApiAddressStatus) -> Self {
        match value {
            ApiAddressStatus::Disabled => Self::Disabled,
            ApiAddressStatus::Enabled => Self::Enabled,
            ApiAddressStatus::Deleting => Self::Deleting,
        }
    }
}

impl FromSql for AddressStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Disabled),
            1 => Ok(Self::Enabled),
            2 => Ok(Self::Deleting),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for AddressStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

impl From<ApiAddressType> for AddressType {
    fn from(value: ApiAddressType) -> Self {
        match value {
            ApiAddressType::Original => Self::Original,
            ApiAddressType::Alias => Self::Alias,
            ApiAddressType::Custom => Self::Custom,
            ApiAddressType::Premium => Self::Premium,
            ApiAddressType::External => Self::External,
        }
    }
}

impl FromSql for AddressType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(Self::Original),
            2 => Ok(Self::Alias),
            3 => Ok(Self::Custom),
            4 => Ok(Self::Premium),
            5 => Ok(Self::External),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for AddressType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum CardType {
    /// TODO: Document this variant.
    ClearText = 0,

    /// TODO: Document this variant.
    Encrypted = 1,

    /// TODO: Document this variant.
    Signed = 2,

    /// TODO: Document this variant.
    EncryptedAndSigned = 3,
}

impl From<ApiCardType> for CardType {
    fn from(value: ApiCardType) -> Self {
        match value {
            ApiCardType::ClearText => Self::ClearText,
            ApiCardType::Encrypted => Self::Encrypted,
            ApiCardType::Signed => Self::Signed,
            ApiCardType::EncryptedAndSigned => Self::EncryptedAndSigned,
        }
    }
}

impl FromSql for CardType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::ClearText),
            1 => Ok(Self::Encrypted),
            2 => Ok(Self::Signed),
            3 => Ok(Self::EncryptedAndSigned),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for CardType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ContactSendingPreferences {
    /// TODO: Document this variant.
    Custom = 0,

    /// TODO: Document this variant.
    Default = 1,
}

impl From<ApiContactSendingPreferences> for ContactSendingPreferences {
    fn from(value: ApiContactSendingPreferences) -> Self {
        match value {
            ApiContactSendingPreferences::Custom => Self::Custom,
            ApiContactSendingPreferences::Default => Self::Default,
        }
    }
}

impl FromSql for ContactSendingPreferences {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Custom),
            1 => Ok(Self::Default),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for ContactSendingPreferences {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

impl From<ApiDateFormat> for DateFormat {
    fn from(value: ApiDateFormat) -> Self {
        match value {
            ApiDateFormat::Default => Self::Default,
            ApiDateFormat::DdMmYyyy => Self::DdMmYyyy,
            ApiDateFormat::MmDdYyyy => Self::MmDdYyyy,
            ApiDateFormat::YyyyMmDd => Self::YyyyMmDd,
        }
    }
}

impl FromSql for DateFormat {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Default),
            1 => Ok(Self::DdMmYyyy),
            2 => Ok(Self::MmDdYyyy),
            3 => Ok(Self::YyyyMmDd),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for DateFormat {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Density {
    /// TODO: Document this variant.
    Comfortable = 0,

    /// TODO: Document this variant.
    Compact = 1,
}

impl From<ApiDensity> for Density {
    fn from(value: ApiDensity) -> Self {
        match value {
            ApiDensity::Comfortable => Self::Comfortable,
            ApiDensity::Compact => Self::Compact,
        }
    }
}

impl FromSql for Density {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Comfortable),
            1 => Ok(Self::Compact),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for Density {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum EarlyAccess {
    /// TODO: Document this variant.
    Regular = 0,

    /// TODO: Document this variant.
    Beta = 1,
}

impl From<ApiEarlyAccess> for EarlyAccess {
    fn from(value: ApiEarlyAccess) -> Self {
        match value {
            ApiEarlyAccess::Regular => Self::Regular,
            ApiEarlyAccess::Beta => Self::Beta,
        }
    }
}

impl FromSql for EarlyAccess {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Regular),
            1 => Ok(Self::Beta),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for EarlyAccess {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum LightOrDarkMode {
    /// TODO: Document this variant.
    Light = 0,

    /// TODO: Document this variant.
    Dark = 1,
}

impl From<ApiLightOrDarkMode> for LightOrDarkMode {
    fn from(value: ApiLightOrDarkMode) -> Self {
        match value {
            ApiLightOrDarkMode::Light => Self::Light,
            ApiLightOrDarkMode::Dark => Self::Dark,
        }
    }
}

impl From<LightOrDarkMode> for ApiLightOrDarkMode {
    fn from(value: LightOrDarkMode) -> Self {
        match value {
            LightOrDarkMode::Light => Self::Light,
            LightOrDarkMode::Dark => Self::Dark,
        }
    }
}

impl FromSql for LightOrDarkMode {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Light),
            1 => Ok(Self::Dark),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for LightOrDarkMode {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum LogAuth {
    /// TODO: Document this variant.
    Disabled = 0,

    /// TODO: Document this variant.
    Basic = 1,

    /// TODO: Document this variant.
    Advanced = 2,
}

impl From<ApiLogAuth> for LogAuth {
    fn from(value: ApiLogAuth) -> Self {
        match value {
            ApiLogAuth::Disabled => Self::Disabled,
            ApiLogAuth::Basic => Self::Basic,
            ApiLogAuth::Advanced => Self::Advanced,
        }
    }
}

impl FromSql for LogAuth {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Disabled),
            1 => Ok(Self::Basic),
            2 => Ok(Self::Advanced),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for LogAuth {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[repr(u8)]
pub enum TfaStatus {
    /// TODO: Document this variant.
    #[default]
    None = 0,

    /// TODO: Document this variant.
    Totp = 1,

    /// TODO: Document this variant.
    Fido2 = 2,

    /// TODO: Document this variant.
    TotpOrFido2 = 3,
}

impl From<ApiTfaStatus> for TfaStatus {
    fn from(value: ApiTfaStatus) -> Self {
        match value {
            ApiTfaStatus::None => Self::None,
            ApiTfaStatus::Totp => Self::Totp,
            ApiTfaStatus::Fido2 => Self::Fido2,
            ApiTfaStatus::TotpOrFido2 => Self::TotpOrFido2,
        }
    }
}

impl FromSql for TfaStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::None),
            1 => Ok(Self::Totp),
            2 => Ok(Self::Fido2),
            3 => Ok(Self::TotpOrFido2),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for TfaStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum TimeFormat {
    /// TODO: Document this variant.
    Default = 0,

    /// TODO: Document this variant.
    H24 = 1,

    /// TODO: Document this variant.
    H12 = 2,
}

impl From<ApiTimeFormat> for TimeFormat {
    fn from(value: ApiTimeFormat) -> Self {
        match value {
            ApiTimeFormat::Default => Self::Default,
            ApiTimeFormat::H24 => Self::H24,
            ApiTimeFormat::H12 => Self::H12,
        }
    }
}

impl FromSql for TimeFormat {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Default),
            1 => Ok(Self::H24),
            2 => Ok(Self::H12),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for TimeFormat {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

impl From<ApiUserMnemonicStatus> for UserMnemonicStatus {
    fn from(value: ApiUserMnemonicStatus) -> Self {
        match value {
            ApiUserMnemonicStatus::Disabled => Self::Disabled,
            ApiUserMnemonicStatus::EnabledButNotSet => Self::EnabledButNotSet,
            ApiUserMnemonicStatus::EnabledNeedsReactivation => Self::EnabledNeedsReactivation,
            ApiUserMnemonicStatus::EnabledAndSet => Self::EnabledAndSet,
            ApiUserMnemonicStatus::Unknown => Self::Unknown,
        }
    }
}

impl FromSql for UserMnemonicStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Disabled),
            1 => Ok(Self::EnabledButNotSet),
            2 => Ok(Self::EnabledNeedsReactivation),
            3 => Ok(Self::EnabledAndSet),
            4 => Ok(Self::Unknown),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for UserMnemonicStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum UserType {
    /// TODO: Document this variant.
    Proton = 1,

    /// TODO: Document this variant.
    Managed = 2,

    /// TODO: Document this variant.
    External = 3,
}

impl From<ApiUserType> for UserType {
    fn from(value: ApiUserType) -> Self {
        match value {
            ApiUserType::Proton => Self::Proton,
            ApiUserType::Managed => Self::Managed,
            ApiUserType::External => Self::External,
        }
    }
}

impl FromSql for UserType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(Self::Proton),
            2 => Ok(Self::Managed),
            3 => Ok(Self::External),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for UserType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

impl From<ApiWeekStart> for WeekStart {
    fn from(value: ApiWeekStart) -> Self {
        match value {
            ApiWeekStart::Default => Self::Default,
            ApiWeekStart::Monday => Self::Monday,
            ApiWeekStart::Saturday => Self::Saturday,
            ApiWeekStart::Sunday => Self::Sunday,
        }
    }
}

impl FromSql for WeekStart {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            0 => Ok(Self::Default),
            1 => Ok(Self::Monday),
            6 => Ok(Self::Saturday),
            7 => Ok(Self::Sunday),
            v => Err(FromSqlError::OutOfRange(i64::from(v))),
        }
    }
}

impl ToSql for WeekStart {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

//  STRUCTS
//==============================================================================

/// Wrapper type around [`RealAddressKeys`] to implement [`FromSql`] and
/// [`ToSql`].
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
        Ok(AddressKeys(RealAddressKeys::deserialize(deserializer)?))
    }
}

impl From<RealAddressKeys> for AddressKeys {
    fn from(value: RealAddressKeys) -> Self {
        Self(value)
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

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AddressSignedKeyList {
    /// TODO: Document this field.
    pub data: Option<String>,

    /// TODO: Document this field.
    pub expected_min_epoch_id: Option<u64>,

    /// TODO: Document this field.
    pub max_epoch_id: Option<u64>,

    /// TODO: Document this field.
    pub min_epoch_id: Option<u64>,

    /// TODO: Document this field.
    pub obsolescence_token: Option<String>,

    /// TODO: Document this field.
    pub revision: u64,

    /// TODO: Document this field.
    pub signature: Option<String>,
}

impl From<ApiAddressSignedKeyList> for AddressSignedKeyList {
    fn from(value: ApiAddressSignedKeyList) -> Self {
        Self {
            data: value.data,
            expected_min_epoch_id: value.expected_min_epoch_id,
            max_epoch_id: value.max_epoch_id,
            min_epoch_id: value.min_epoch_id,
            obsolescence_token: value.obsolescence_token,
            revision: value.revision,
            signature: value.signature,
        }
    }
}

sql_using_serde!(AddressSignedKeyList);

/// Wrapper type around `Vec<String>` to implement [`FromSql`] and [`ToSql`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContactTypes(Vec<String>);

impl ContactTypes {
    /// Create a new [`ContactTypes`] instance from a list of [`String`]s.
    ///
    /// # Parameters
    ///
    /// * `types` - The types to wrap.
    ///
    #[must_use]
    pub fn new(types: Vec<String>) -> Self {
        Self(types)
    }

    /// Convert the [`ContactTypes`] into the inner [`Vec`].
    #[must_use]
    pub fn into_inner(self) -> Vec<String> {
        self.0
    }
}

impl Deref for ContactTypes {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

sql_using_serde!(ContactTypes);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Email {
    /// TODO: Document this field.
    pub notify: u8,

    /// TODO: Document this field.
    pub reset: u8,

    /// TODO: Document this field.
    pub status: u8,

    /// TODO: Document this field.
    pub value: String,
}

impl From<ApiEmail> for Email {
    fn from(value: ApiEmail) -> Self {
        Self {
            notify: value.notify,
            reset: value.reset,
            status: value.status,
            value: value.value,
        }
    }
}

sql_using_serde!(Email);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FidoKey {
    /// TODO: Document this field.
    pub attestation_format: String,

    /// TODO: Document this field.
    pub credential_id: Vec<i32>,

    /// TODO: Document this field.
    pub name: String,
}

impl From<ApiFidoKey> for FidoKey {
    fn from(value: ApiFidoKey) -> Self {
        Self {
            attestation_format: value.attestation_format,
            credential_id: value.credential_id,
            name: value.name,
        }
    }
}

sql_using_serde!(FidoKey);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Flags {
    /// TODO: Document this field.
    pub has_temporary_password: bool,

    /// TODO: Document this field.
    pub no_login: bool,

    /// TODO: Document this field.
    pub no_proton_address: bool,

    /// TODO: Document this field.
    pub onboard_checklist_storage_granted: bool,

    /// TODO: Document this field.
    pub protected: bool,

    /// TODO: Document this field.
    pub recovery_attempt: bool,

    /// TODO: Document this field.
    pub sso: bool,

    /// TODO: Document this field.
    pub test_account: bool,
}

impl From<ApiFlags> for Flags {
    fn from(value: ApiFlags) -> Self {
        Self {
            has_temporary_password: value.has_temporary_password,
            no_login: value.no_login,
            no_proton_address: value.no_proton_address,
            onboard_checklist_storage_granted: value.onboard_checklist_storage_granted,
            protected: value.protected,
            recovery_attempt: value.recovery_attempt,
            sso: value.sso,
            test_account: value.test_account,
        }
    }
}

sql_using_serde!(Flags);

/// Wrapper type around `RemoteId` to implement label-specific functionality.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct LabelId(RemoteId);

impl LabelId {
    /// Create a new [`LabelId`] instance from a [`String`].
    ///
    /// # Parameters
    ///
    /// * `id` - The ID to wrap.
    ///
    #[must_use]
    pub fn new(id: String) -> Self {
        Self(RemoteId::new(id))
    }

    /// Convert the [`LabelId`] into the inner [`RemoteId`].
    #[must_use]
    pub fn into_inner(self) -> RemoteId {
        self.0
    }
}

impl Deref for LabelId {
    type Target = RemoteId;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for LabelId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ApiRemoteId> for LabelId {
    fn from(value: ApiRemoteId) -> Self {
        Self(RemoteId::from(value.into_inner()))
    }
}

impl From<LabelId> for ApiRemoteId {
    fn from(value: LabelId) -> Self {
        Self::new(value.into_inner().into_inner())
    }
}

impl From<LabelId> for RemoteId {
    fn from(value: LabelId) -> Self {
        value.into_inner()
    }
}

impl From<RemoteId> for LabelId {
    fn from(value: RemoteId) -> Self {
        Self::from(value.into_inner())
    }
}

impl From<String> for LabelId {
    fn from(id: String) -> Self {
        Self(RemoteId::new(id))
    }
}

impl From<&str> for LabelId {
    fn from(id: &str) -> Self {
        Self(RemoteId::new(id.to_owned()))
    }
}

impl FromSql for LabelId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        String::column_result(value).map(LabelId::from)
    }
}

impl ToSql for LabelId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

/// Wrapper type around `Vec<RemoteId>` to implement [`FromSql`] and [`ToSql`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Labels(Vec<LabelId>);

impl Labels {
    /// Create a new [`Labels`] instance from a list of [`LabelId`]s.
    ///
    /// # Parameters
    ///
    /// * `ids` - The IDs to wrap.
    ///
    #[must_use]
    pub fn new(ids: Vec<LabelId>) -> Self {
        Self(ids)
    }

    /// Convert the [`Labels`] into the inner [`Vec`].
    #[must_use]
    pub fn into_inner(self) -> Vec<LabelId> {
        self.0
    }
}

impl Deref for Labels {
    type Target = Vec<LabelId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

sql_using_serde!(Labels);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HighSecurity {
    /// TODO: Document this field.
    pub eligible: bool,

    /// TODO: Document this field.
    pub value: bool,
}

impl From<ApiHighSecurity> for HighSecurity {
    fn from(value: ApiHighSecurity) -> Self {
        Self {
            eligible: value.eligible,
            value: value.value,
        }
    }
}

sql_using_serde!(HighSecurity);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Password {
    /// TODO: Document this field.
    pub mode: u32,

    /// TODO: Document this field.
    pub expiration_time: Option<u64>,
}

impl From<ApiPassword> for Password {
    fn from(value: ApiPassword) -> Self {
        Self {
            mode: value.mode,
            expiration_time: value.expiration_time,
        }
    }
}

sql_using_serde!(Password);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Phone {
    /// TODO: Document this field.
    pub notify: u8,

    /// TODO: Document this field.
    pub reset: u8,

    /// TODO: Document this field.
    pub status: u8,

    /// TODO: Document this field.
    pub value: String,
}

impl From<ApiPhone> for Phone {
    fn from(value: ApiPhone) -> Self {
        Self {
            notify: value.notify,
            reset: value.reset,
            status: value.status,
            value: value.value,
        }
    }
}

sql_using_serde!(Phone);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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

impl From<ApiProductUsedSpace> for ProductUsedSpace {
    fn from(value: ApiProductUsedSpace) -> Self {
        Self {
            calendar: value.calendar,
            contact: value.contact,
            drive: value.drive,
            mail: value.mail,
            pass: value.pass,
        }
    }
}

sql_using_serde!(ProductUsedSpace);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Referral {
    /// TODO: Document this field.
    pub eligible: bool,

    /// TODO: Document this field.
    pub link: String,
}

impl From<ApiReferral> for Referral {
    fn from(value: ApiReferral) -> Self {
        Self {
            eligible: value.eligible,
            link: value.link,
        }
    }
}

sql_using_serde!(Referral);

/// Remote ID.
///
/// This minimal struct is simply a wrapper around a [`String`], and is used to
/// formalise all IDs used by the Proton API.
///
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RemoteId(String);

impl RemoteId {
    /// Create a new [`RemoteId`] from a [`String`].
    ///
    /// # Parameters
    ///
    /// * `id` - The ID to wrap.
    ///
    #[must_use]
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Convert the [`RemoteId`] into the inner [`String`].
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl CloneableSecret for RemoteId {}

impl DebugSecret for RemoteId {}

impl Deref for RemoteId {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for RemoteId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ApiRemoteId> for RemoteId {
    fn from(value: ApiRemoteId) -> Self {
        Self(value.into_inner())
    }
}

impl From<RemoteId> for ApiRemoteId {
    fn from(value: RemoteId) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<String> for RemoteId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for RemoteId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl FromSql for RemoteId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        String::column_result(value).map(RemoteId)
    }
}

impl ToSql for RemoteId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

impl Zeroize for RemoteId {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettingsFlags {
    /// TODO: Document this field.
    pub in_app_promos_hidden: bool,

    /// TODO: Document this field.
    pub welcomed: bool,
}

impl From<ApiSettingsFlags> for SettingsFlags {
    fn from(value: ApiSettingsFlags) -> Self {
        Self {
            in_app_promos_hidden: value.in_app_promos_hidden,
            welcomed: value.welcomed,
        }
    }
}

sql_using_serde!(SettingsFlags);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TwoFa {
    /// TODO: Document this field.
    pub allowed: TfaStatus,

    /// TODO: Document this field.
    pub enabled: TfaStatus,

    /// TODO: Document this field.
    pub expiration_time: Option<u64>,

    /// TODO: Document this field.
    pub registered_keys: Vec<FidoKey>,
}

impl From<ApiTwoFa> for TwoFa {
    fn from(value: ApiTwoFa) -> Self {
        Self {
            allowed: value.allowed.into(),
            enabled: value.enabled.into(),
            expiration_time: value.expiration_time,
            registered_keys: value
                .registered_keys
                .into_iter()
                .map(FidoKey::from)
                .collect(),
        }
    }
}

sql_using_serde!(TwoFa);

/// Wrapper type around [`RealUserKeys`] to implement [`FromSql`] and [`ToSql`].
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
        Ok(UserKeys(RealUserKeys::deserialize(deserializer)?))
    }
}

impl From<RealUserKeys> for UserKeys {
    fn from(value: RealUserKeys) -> Self {
        Self(value)
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
