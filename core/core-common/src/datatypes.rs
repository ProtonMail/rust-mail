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
//! Hence event data types are placed into the [`events`](crate::user_context::event_loop::events) module.
//!

mod account_details;
mod avatar;
pub mod contact_details;
mod contact_list;
mod dependencies;
mod issue_report;
mod push_notifications;
mod system_label;
mod timestamp;
mod user_feature_flags;

pub use self::account_details::AccountDetails;
pub use self::avatar::AvatarInformation;
pub use self::contact_list::*;
pub use self::dependencies::*;
pub use self::issue_report::*;
pub use self::push_notifications::*;
pub use self::system_label::*;
pub use self::timestamp::*;
pub use self::user_feature_flags::*;

use bitflags::bitflags;
use derive_more::Into;
use derive_more::derive::TryFrom;
use itertools::Itertools;
use jiff::civil::Weekday;
use proton_core_api::services::proton::muon::rt::DynResolver;
use proton_core_api::services::proton::{
    AddressFlags as ApiAddressFlags, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
    ContactSendingPreferences as ApiContactSendingPreferences, DateFormat as ApiDateFormat,
    Density as ApiDensity, EarlyAccess as ApiEarlyAccess, Email as ApiEmail, FidoKey as ApiFidoKey,
    Flags as ApiFlags, HighSecurity as ApiHighSecurity, LogAuth as ApiLogAuth,
    Password as ApiPassword, PasswordMode as ApiPasswordMode, Phone as ApiPhone,
    ProductUsedSpace as ApiProductUsedSpace, Referral as ApiReferral,
    SettingsFlags as ApiSettingsFlags, TfaStatus as ApiTfaStatus, TimeFormat as ApiTimeFormat,
    TwoFa as ApiTwoFa, UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
    WeekStart as ApiWeekStart,
};
use proton_core_api::services::proton::{
    AddressId, ContactEmailId, ContactId, DeviceEnvironment as ApiDeviceEnvironment, LabelId,
    LabelType as ApiLabelType, LightOrDarkMode as ApiLightOrDarkMode,
};
use proton_core_api::session::{Config as RealApiConfig, EnvId};
use proton_core_api::store::{MbpMode, TfaMode};
use proton_crypto_account::keys::{AddressKeys as RealAddressKeys, UserKeys as RealUserKeys};
use proton_sqlite3::rusqlite::Error as SqlError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smart_default::SmartDefault;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::utils::sql_using_serde;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for AddressStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressFlags(pub u32);

bitflags! {
    impl AddressFlags: u32 {
        const BYOE = 1 << 6;

        // Safeguard against unknown values
        const _ = !0;
    }
}

impl AddressFlags {
    #[must_use]
    pub fn is_byoe(&self) -> bool {
        self.contains(Self::BYOE)
    }
}

impl Default for AddressFlags {
    fn default() -> Self {
        ApiAddressFlags::default().into()
    }
}

impl From<ApiAddressFlags> for AddressFlags {
    fn from(value: ApiAddressFlags) -> Self {
        Self(value.0)
    }
}
impl From<AddressFlags> for ApiAddressFlags {
    fn from(value: AddressFlags) -> Self {
        Self(value.0)
    }
}

impl FromSql for AddressFlags {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(Self(u32::column_result(value)?))
    }
}

impl ToSql for AddressFlags {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(i64::from(self.0))))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Refresh {
    None,
    Mail,
    Contacts,
    All,
    Unknown(u8),
}

impl Refresh {
    #[must_use]
    pub fn is_refresh(&self) -> bool {
        !matches!(self, Refresh::None)
    }
}

impl From<u8> for Refresh {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Mail,
            2 => Self::Contacts,
            255 => Self::All,
            other => Self::Unknown(other),
        }
    }
}

impl From<Refresh> for u8 {
    fn from(value: Refresh) -> Self {
        match value {
            Refresh::None => 0,
            Refresh::Mail => 1,
            Refresh::Contacts => 2,
            Refresh::All => 255,
            Refresh::Unknown(other) => other,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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

impl AddressType {
    #[must_use]
    pub fn is_external(&self) -> bool {
        matches!(self, Self::External)
    }
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for AddressType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ContactSendingPreferences {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for DateFormat {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for Density {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for EarlyAccess {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for LightOrDarkMode {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for LogAuth {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize, TryFrom)]
#[try_from(repr)]
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
impl TfaStatus {
    /// Returns true if any type of second factor auth method is active.
    #[must_use]
    pub fn has_tfa(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns true if TOTP is enabled.
    #[must_use]
    pub fn has_totp(self) -> bool {
        matches!(self, Self::Totp | Self::TotpOrFido2)
    }

    /// Returns true if FIDO2 is enabled.
    #[must_use]
    pub fn has_fido(self) -> bool {
        matches!(self, Self::Fido2 | Self::TotpOrFido2)
    }
}

impl From<TfaMode> for TfaStatus {
    fn from(value: TfaMode) -> Self {
        match (value.totp, value.fido) {
            (true, true) => Self::TotpOrFido2,
            (true, false) => Self::Totp,
            (false, true) => Self::Fido2,
            (false, false) => Self::None,
        }
    }
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

impl From<TfaStatus> for ApiTfaStatus {
    fn from(value: TfaStatus) -> Self {
        match value {
            TfaStatus::None => Self::None,
            TfaStatus::Totp => Self::Totp,
            TfaStatus::Fido2 => Self::Fido2,
            TfaStatus::TotpOrFido2 => Self::TotpOrFido2,
        }
    }
}

impl FromSql for TfaStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for TfaStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom, SmartDefault)]
#[try_from(repr)]
#[repr(u8)]
pub enum TimeFormat {
    /// TODO: Document this variant.
    #[default]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for TimeFormat {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom, SmartDefault)]
#[try_from(repr)]
#[repr(u8)]
pub enum UserMnemonicStatus {
    /// TODO: Document this variant.
    #[default]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for UserMnemonicStatus {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom, SmartDefault)]
#[try_from(repr)]
#[repr(u8)]
pub enum UserType {
    /// TODO: Document this variant.
    #[default]
    Proton = 1,

    /// TODO: Document this variant.
    Managed = 2,

    /// TODO: Document this variant.
    External = 3,

    /// Credentialles user
    CredentialLess = 4,

    Unknown(u8),
}

impl From<UserType> for i64 {
    fn from(value: UserType) -> Self {
        match value {
            UserType::Proton => 1,
            UserType::Managed => 2,
            UserType::External => 3,
            UserType::CredentialLess => 4,
            UserType::Unknown(v) => i64::from(v),
        }
    }
}

impl From<ApiUserType> for UserType {
    fn from(value: ApiUserType) -> Self {
        match value {
            ApiUserType::Proton => Self::Proton,
            ApiUserType::Managed => Self::Managed,
            ApiUserType::External => Self::External,
            ApiUserType::CredentialLess => Self::CredentialLess,
            ApiUserType::Unknown(v) => {
                warn!("Detected `Unknown` user type: {}", v);
                Self::Unknown(v)
            }
        }
    }
}

impl FromSql for UserType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for UserType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer((*self).into())))
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
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
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for WeekStart {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl From<WeekStart> for Weekday {
    fn from(value: WeekStart) -> Self {
        match value {
            WeekStart::Default | WeekStart::Monday => Weekday::Monday,
            WeekStart::Saturday => Weekday::Saturday,
            WeekStart::Sunday => Weekday::Sunday,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum LabelType {
    /// TODO: Document this field.
    Label = 1,

    /// TODO: Document this field.
    ContactGroup = 2,

    /// TODO: Document this field.
    Folder = 3,

    /// TODO: Document this field.
    System = 4,
}

impl Display for LabelType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Label => write!(f, "Label"),
            Self::ContactGroup => write!(f, "Contact Group"),
            Self::Folder => write!(f, "Folder"),
            Self::System => write!(f, "System"),
        }
    }
}

impl From<ApiLabelType> for LabelType {
    fn from(value: ApiLabelType) -> Self {
        match value {
            ApiLabelType::Label => Self::Label,
            ApiLabelType::ContactGroup => Self::ContactGroup,
            ApiLabelType::Folder => Self::Folder,
            ApiLabelType::System => Self::System,
        }
    }
}

impl From<LabelType> for ApiLabelType {
    fn from(value: LabelType) -> Self {
        match value {
            LabelType::Label => Self::Label,
            LabelType::ContactGroup => Self::ContactGroup,
            LabelType::Folder => Self::Folder,
            LabelType::System => Self::System,
        }
    }
}

impl FromSql for LabelType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for LabelType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

pub const ALL_LABEL_TYPES: [LabelType; 4] = [
    LabelType::Label,
    LabelType::ContactGroup,
    LabelType::Folder,
    LabelType::System,
];
pub const MAIL_LABEL_TYPES: [LabelType; 3] =
    [LabelType::Label, LabelType::Folder, LabelType::System];
pub const CONTACT_LABEL_TYPES: [LabelType; 1] = [LabelType::ContactGroup];

/// In which environment are we going to register the device
/// for the push notification.
///
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum DeviceEnvironment {
    Google = 4,
    AppleProd = 6,
    AppleBeta = 7,
    AppleProdET = 14,
    AppleDevET = 15,
    AppleDev = 16,
}

impl FromSql for DeviceEnvironment {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for DeviceEnvironment {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl From<DeviceEnvironment> for ApiDeviceEnvironment {
    fn from(value: DeviceEnvironment) -> Self {
        match value {
            DeviceEnvironment::Google => Self::Google,
            DeviceEnvironment::AppleProd => Self::AppleProd,
            DeviceEnvironment::AppleBeta => Self::AppleBeta,
            DeviceEnvironment::AppleProdET => Self::AppleProdET,
            DeviceEnvironment::AppleDevET => Self::AppleDevET,
            DeviceEnvironment::AppleDev => Self::AppleDev,
        }
    }
}

/// Key used to distinguish between components in the initialization.
/// It is a string, not an enum for making it open for additional changes from different BU.
///
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct InitializationKey(pub &'static str);

impl InitializationKey {
    #[must_use]
    pub const fn new(s: &'static str) -> Self {
        Self(s)
    }
}

impl From<InitializationKey> for String {
    fn from(value: InitializationKey) -> Self {
        value.0.to_owned()
    }
}

/// State in which component is in the initialization.
/// Used to determine if something was already initialized or not.
///
#[derive(Clone, Copy, Debug, PartialEq, Eq, TryFrom, Default)]
#[try_from(repr)]
#[repr(u8)]
pub enum InitializedComponentState {
    /// Component needs initializing. Default state.
    #[default]
    NotInitialized = 0,
    /// Component failed to initialize. Can cascade on other components.
    Failed = 1,
    /// Component has been sucesfully initialized. Does not require repetetive
    /// initialization.
    Succeeded = 2,
}

impl FromSql for InitializedComponentState {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for InitializedComponentState {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

//  STRUCTS
//==============================================================================

/// Wrapper type around [`RealAddressKeys`] to implement [`FromSql`] and
/// [`ToSql`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressKeys(pub RealAddressKeys);

impl Default for AddressKeys {
    fn default() -> Self {
        Self(RealAddressKeys::new(vec![]))
    }
}

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

#[derive(Clone)]
pub struct AppDetails {
    /// Example: "ios"
    pub platform: String,
    /// Example: "mail"
    pub product: String,
    /// Example: "1.0.0"
    pub version: String,
}

/// Note, this is almost identical to [`proton_core_api::session::Config`], however instead of storing
/// concatenated `app_version`, it keeps [`AppDetails`] instead.
#[derive(Clone)]
pub struct ApiConfig {
    pub app_details: AppDetails,
    pub user_agent: Option<String>,
    pub env_id: EnvId,
    pub proxy: Option<String>,
    pub resolver: Option<DynResolver>,
}

impl ApiConfig {
    /// Extracts the client id from the app version, which usually looks like "platform-app@version", eg.: android-mail@10.9
    #[must_use]
    pub fn get_client_id(&self) -> String {
        format!("{}-{}", self.app_details.platform, self.app_details.product)
    }
}

impl From<ApiConfig> for RealApiConfig {
    fn from(config: ApiConfig) -> Self {
        let AppDetails {
            platform,
            product,
            version,
        } = config.app_details;
        Self {
            app_version: proton_core_api::session::format_api_app_version(
                &platform, &product, &version,
            ),
            user_agent: config.user_agent,
            env_id: config.env_id,
            proxy: config.proxy,
            resolver: config.resolver,
        }
    }
}

/// Wrapper type around `Vec<String>` to implement [`FromSql`] and [`ToSql`].
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContactTypes(Vec<String>);

impl ContactTypes {
    /// Create a new [`ContactTypes`] instance from a list of [`String`]s.
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

/// Wrapper type around `Vec<RemoteId>` to implement [`FromSql`] and [`ToSql`].
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Labels(Vec<LabelId>);

impl Labels {
    /// Create a new [`Labels`] instance from a list of [`LabelId`]s.
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

/// Marker trait to signal that this type was declared as a local id.
pub trait LocalIdMarker: Sized {
    type Counterpart: Clone + Send + Sync + ToSql + FromSql + Serialize + DeserializeOwned + Debug;
}

/// Declare a new Local id type that maps to a remote Proton Id.
///
/// A local identifier should exist for every remote/proton Id for every resource we store
/// in the database that we will create/mutate.
///
/// # Example
///
/// ```
/// use proton_core_api::declare_proton_id;
/// use proton_core_common::declare_local_id;
///
/// declare_proton_id!(pub MyProtonId);
/// declare_local_id!(pub MyLocalProtonId => MyProtonId);
/// ```
#[macro_export]
macro_rules! declare_local_id {
    (
        $(#[$($attrss:tt)*])*
        $visibility:vis $name:ident => $remote_id:ident
    ) => {

        $(#[$($attrss)*])*
        #[derive(Clone, Copy, Debug, serde::Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize)]
        pub struct $name(u64);

        impl $name {
            /// Represents the internal value as an unsigned 64-bit integer.
            #[must_use]
            pub const fn as_u64(&self) -> u64 {
                self.0
            }
        }

        impl AsRef<u64> for $name{
            fn as_ref(&self) -> &u64 {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u64> for $name{
            fn from(id: u64) -> Self {
                Self(id)
            }
        }

        impl ::stash::exports::FromSql for $name {
            fn column_result(value: ::stash::exports::ValueRef<'_>) -> ::stash::exports::FromSqlResult<Self> {
                u64::column_result(value).map($name)
            }
        }

        impl ::stash::exports::ToSql for $name {
            fn to_sql(&self) -> Result<::stash::exports::ToSqlOutput<'_>, ::stash::exports::SqliteError> {
                self.0.to_sql()
            }
        }

        impl $crate::datatypes::LocalIdMarker for $name {
            type Counterpart = $remote_id;
        }

        impl $crate::actions::dependency_builder::LocalIdActionDepExt for $name {
            fn to_dependency_key(&self) -> ::proton_action_queue::action::ActionDependencyKey {
                ::proton_action_queue::action::ActionDependencyKey::from(format!("dep-{}-{}",stringify!($name), self.0))
            }

            fn to_create_dependency_key(&self) -> ::proton_action_queue::action::ActionDependencyKey {
                ::proton_action_queue::action::ActionDependencyKey::from(format!("create-{}-{}",stringify!($name), self.0))
            }

            fn to_custom_dependency_key(&self, prefix:&str) -> ::proton_action_queue::action::ActionDependencyKey {
                ::proton_action_queue::action::ActionDependencyKey::from(format!("{prefix}-{}-{}",stringify!($name), self.0))
            }
        }

        impl From<$name> for ::proton_action_queue::rebase::RebaseKey {
            fn from(id: $name) -> Self {
                ::proton_action_queue::rebase::RebaseKey::from(format!("{}-{}", stringify!($name), id.0))
            }
        }
    };
}

declare_local_id!(LocalContactId => ContactId);
declare_local_id!(LocalContactEmailId => ContactEmailId);
declare_local_id!(LocalAddressId => AddressId);
declare_local_id!(LocalLabelId => LabelId);

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
            mode: value.mode as u32,
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

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettingsFlags {
    /// TODO: Document this field.
    pub welcomed: bool,
    /// `EasyDeviceMigration` (QR Login) opt out. The user can choose to disable the feature.
    pub edm_opt_out: bool,
}

impl From<ApiSettingsFlags> for SettingsFlags {
    fn from(value: ApiSettingsFlags) -> Self {
        Self {
            welcomed: value.welcomed,
            edm_opt_out: value.edm_opt_out,
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

/// Wrapper type around `Vec<String>` to hold the auth scope(s) of a session.
///
/// TODO: Use a `HashSet`?
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuthScopes(Vec<String>);

impl AuthScopes {
    /// Create a new [`AuthScopes`] instance from a list of [`String`]s.
    ///
    /// TODO: Might be better to have a `From<Vec<String>>` implementation.
    ///
    #[must_use]
    pub fn new(scopes: impl IntoIterator<Item: Into<String>>) -> Self {
        Self(scopes.into_iter().map_into().collect())
    }

    /// Returns true if the [`AuthScopes`] contains the specified scope.
    #[must_use]
    pub fn contains(&self, scope: &str) -> bool {
        self.0.iter().any(|s| s == scope)
    }

    /// Convert the [`AuthScopes`] into the inner [`Vec`].
    #[must_use]
    pub fn into_inner(self) -> Vec<String> {
        self.0
    }
}

sql_using_serde!(AuthScopes);

/// A compat type for the [`ApiPasswordMode`] enum, enabling it to be used
/// within the database.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum PasswordMode {
    #[default]
    One = 1,
    Two = 2,
}

impl PasswordMode {
    /// Returns true if any type of additional password is active.
    #[must_use]
    pub fn has_mbp(self) -> bool {
        !matches!(self, Self::One)
    }
}

impl From<MbpMode> for PasswordMode {
    fn from(value: MbpMode) -> Self {
        match value {
            MbpMode::One => Self::One,
            MbpMode::Two => Self::Two,
        }
    }
}

impl From<PasswordMode> for MbpMode {
    fn from(value: PasswordMode) -> Self {
        match value {
            PasswordMode::One => MbpMode::One,
            PasswordMode::Two => MbpMode::Two,
        }
    }
}

impl From<ApiPasswordMode> for PasswordMode {
    fn from(value: ApiPasswordMode) -> Self {
        match value {
            ApiPasswordMode::One => Self::One,
            ApiPasswordMode::Two => Self::Two,
        }
    }
}

impl From<PasswordMode> for ApiPasswordMode {
    fn from(value: PasswordMode) -> Self {
        match value {
            PasswordMode::One => ApiPasswordMode::One,
            PasswordMode::Two => ApiPasswordMode::Two,
        }
    }
}

impl ToSql for PasswordMode {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqlError> {
        Ok((*self as u8).into())
    }
}

impl FromSql for PasswordMode {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        let ValueRef::Integer(value) = value else {
            return Err(FromSqlError::InvalidType);
        };

        let Ok(value) = u8::try_from(value) else {
            return Err(FromSqlError::InvalidType);
        };

        let Ok(value) = Self::try_from(value) else {
            return Err(FromSqlError::InvalidType);
        };

        Ok(value)
    }
}

/// Wrapper type around [`RealUserKeys`] to implement [`FromSql`] and [`ToSql`].
#[derive(Clone, Debug, Eq, PartialEq, Into)]
pub struct UserKeys(pub RealUserKeys);

impl Default for UserKeys {
    fn default() -> Self {
        Self(RealUserKeys::new(vec![]))
    }
}

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

/// A simple wrapper around a [`f64`] to represent a timestamp.
///
/// Represents the number of seconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Timestamp(f64);

impl Timestamp {
    /// Create a new [`Timestamp`] at the current time.
    ///
    #[must_use]
    pub fn now() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_secs_f64();

        Self(now)
    }

    /// Returns the inner value as a [`f64`].
    #[must_use]
    pub const fn as_f64(&self) -> f64 {
        self.0
    }
}

impl FromSql for Timestamp {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        f64::column_result(value).map(Timestamp)
    }
}

impl ToSql for Timestamp {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        f64::to_sql(&self.0)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LabelColor(String);

impl LabelColor {
    #[must_use]
    pub fn purple() -> Self {
        Self("#8080FF".into())
    }
    #[must_use]
    pub fn black() -> Self {
        Self("#000000".into())
    }
}

impl Display for LabelColor {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for LabelColor {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for LabelColor {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl FromSql for LabelColor {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_str().map(|s| LabelColor(s.to_string()))
    }
}

impl ToSql for LabelColor {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::from(self.0.clone()))
    }
}

/// This struct is used to registed the device for Push notifications.
///
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredDevice {
    /// Device token, used as primary key
    pub device_token: String,

    /// Environment to which we register
    pub environment: DeviceEnvironment,

    /// TODO: Document this field
    pub ping_notification_status: Option<i32>,

    /// TODO: Document this field
    pub push_notification_status: Option<i32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[repr(transparent)]
pub struct ImageProxy(pub u32);

bitflags! {
    impl ImageProxy: u32 {
        const ENABLED = 2;
    }
}

impl Default for ImageProxy {
    fn default() -> Self {
        Self(2)
    }
}

impl FromSql for ImageProxy {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(ImageProxy(u32::column_result(value)?))
    }
}

impl ToSql for ImageProxy {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        u32::to_sql(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[repr(transparent)]
pub struct NotificationSettings(pub u32);

bitflags! {
    impl NotificationSettings: u32 {
        const ANNOUNCEMENTS = 1 << 0;
        const FEATURES = 1 << 1;
        const NEWSLETTER = 1 << 2;
        const BETA = 1 << 3;
        const BUSINESS = 1 << 4;
        const OFFERS = 1 << 5;
        const NEW_MAIL_NOTIFICATION = 1 << 6;
        const ONBOARDING = 1 << 7;
        const USER_SURVEYS = 1 << 8;
        const PRODUCT_INBOX = 1 << 9;
        const PRODUCT_VPN = 1 << 10;
        const PRODUCT_DRIVE = 1 << 11;
        const PRODUCT_PASS = 1 << 12;
        const PRODUCT_WALLET = 1 << 13;
        const IN_APP_NOTIFICATIONS = 1 << 14;
        const PRODUCT_LUMO = 1 << 15;
    }
}

impl FromSql for NotificationSettings {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(NotificationSettings(u32::column_result(value)?))
    }
}

impl ToSql for NotificationSettings {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        u32::to_sql(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpsellEligibility {
    Eligible(UpsellType),
    NotEligible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpsellType {
    BlackFriday(BlackFridayWave),
    Standard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlackFridayWave {
    /// 50% off
    Wave1,
    /// 80% off
    Wave2,
}

#[cfg(any(test, feature = "test-utils"))]
mod tests {
    use super::{ApiConfig, AppDetails, EnvId};

    impl Default for AppDetails {
        fn default() -> Self {
            Self {
                platform: "ios".to_string(),
                product: "mail".to_string(),
                version: "7.0.1".to_string(),
            }
        }
    }

    impl Default for ApiConfig {
        fn default() -> Self {
            Self {
                app_details: AppDetails::default(),
                user_agent: None,
                env_id: EnvId::new_prod(),
                proxy: None,
                resolver: None,
            }
        }
    }

    impl ApiConfig {
        #[must_use]
        pub fn default_with_env(env_id: EnvId) -> Self {
            Self {
                env_id,
                ..Default::default()
            }
        }
    }
}
