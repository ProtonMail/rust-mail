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

mod avatar;
mod contact_list;

pub use self::avatar::AvatarInformation;
pub use self::contact_list::*;

use core::fmt;
use indoc::formatdoc;
use itertools::Itertools;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use proton_api_core::services::proton::common::LightOrDarkMode as ApiLightOrDarkMode;
use proton_api_core::services::proton::response_data::{
    AddressSignedKeyList as ApiAddressSignedKeyList, AddressStatus as ApiAddressStatus,
    AddressType as ApiAddressType, ContactSendingPreferences as ApiContactSendingPreferences,
    DateFormat as ApiDateFormat, Density as ApiDensity, EarlyAccess as ApiEarlyAccess,
    Email as ApiEmail, FidoKey as ApiFidoKey, Flags as ApiFlags, HighSecurity as ApiHighSecurity,
    LogAuth as ApiLogAuth, Password as ApiPassword, PasswordMode as ApiPasswordMode,
    Phone as ApiPhone, ProductUsedSpace as ApiProductUsedSpace, Referral as ApiReferral,
    SettingsFlags as ApiSettingsFlags, TfaStatus as ApiTfaStatus, TimeFormat as ApiTimeFormat,
    TwoFa as ApiTwoFa, UserMnemonicStatus as ApiUserMnemonicStatus, UserType as ApiUserType,
    WeekStart as ApiWeekStart,
};
use proton_api_core::store::{MbpMode, TfaMode};
use proton_crypto_account::keys::{AddressKeys as RealAddressKeys, UserKeys as RealUserKeys};
use proton_sqlite3::rusqlite::Error as SqlError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Value, ValueRef,
};
use stash::macros::DbRecord;
use stash::orm::Model;
use stash::params;
use stash::stash::{StashError, Tether};
use stash::utils::sql_using_serde;
use std::fmt::{Debug, Display, Formatter};
use std::iter::repeat;
use std::ops::Deref;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

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

impl TfaStatus {
    /// Returns true if any type of second factor auth method is active.
    #[must_use]
    pub fn want_tfa(self) -> bool {
        !matches!(self, Self::None)
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

    Unknown(u8),
}

impl From<UserType> for i64 {
    fn from(value: UserType) -> Self {
        match value {
            UserType::Proton => 1,
            UserType::Managed => 2,
            UserType::External => 3,
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
            ApiUserType::Unknown(v) => {
                warn!("Detected `Unknown` user type: {}", v);
                Self::Unknown(v)
            }
        }
    }
}

impl FromSql for UserType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(Self::Proton),
            2 => Ok(Self::Managed),
            3 => Ok(Self::External),
            v => Ok(Self::Unknown(v)),
        }
    }
}

impl ToSql for UserType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer((*self).into())))
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

//  TRAITS
//==============================================================================

/// Shared functionality associated with a database identifier.
#[allow(async_fn_in_trait)]
pub trait Id: Clone + Send + Sync {
    /// Loads a record from the database by ID.
    ///
    /// This function retrieves a single record from the database by its unique
    /// ID, as an instance of the specified type `T`, where `T` is any concrete
    /// type implementing the [`Model`] trait.
    ///
    /// For full usage details, see [`Model::load()`].
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    /// # See also
    ///
    /// * [`Model::load()`]
    ///
    async fn load<T>(&self, tether: &Tether) -> Result<Option<T>, StashError>
    where
        T: Model;

    /// Name of the id field in the database table.
    fn id_field_name() -> &'static str;
}

/// Mapping trait which allows one to convert a local into a remote id and vice-versa.
#[allow(async_fn_in_trait)]
pub trait IdCounterpart: Clone + Send + Sync {
    /// The counterpart type to this ID.
    type Counterpart: Id;

    /// Identify the counterpart to this ID.
    ///
    /// This function looks up the counterpart to this ID, i.e. if this ID is a
    /// [`LocalId`] then the corresponding [`RemoteId`] is returned, and vice
    /// versa. Note that it does this via database query.
    ///
    /// For full usage details, see [`Model::load()`].
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    /// # See also
    ///
    /// * [`Model::load()`]
    ///
    async fn counterpart<T>(
        &self,
        tether: &Tether,
    ) -> Result<Option<Self::Counterpart>, StashError>
    where
        T: Model;

    /// Obtain the counterparts of a list of IDs.
    ///
    /// This function looks up the counterparts to the specified IDs, i.e. if
    /// the IDs are [`LocalId`]s then the corresponding [`RemoteId`]s are
    /// returned, and vice versa. Note that it does this via database query.
    ///
    /// For full usage details, see [`Model::find()`].
    ///
    /// # Parameters
    ///
    /// * `ids`       - The list of IDs to find the counterparts for.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// See [`Model::find()`].
    ///
    /// # See also
    ///
    /// * [`Model::find()`]
    ///
    async fn counterparts<T>(
        ids: Vec<Self>,
        tether: &Tether,
    ) -> Result<Vec<Self::Counterpart>, StashError>
    where
        T: Model;
}

/// Extension of functionality shared by both [`LocalId`] and [`RemoteId`].
///
/// This trait extends the baseline functionality provided by the [`Id`] trait
/// in order to provide additional functionality that requires implementation
/// to [`AgnosticId`] by enum variant and not to the whole enum generally. At
/// present this is just the [`opt()`](IdOpt::opt) function, which wraps the ID
/// in an [`Option`].
///
pub trait IdOpt<T>: Id
where
    T: Id,
{
    /// Wraps the ID in an [`Option`].
    ///
    /// This function wraps the ID in an [`Option`], returning `Some(id)`. This
    /// is useful for use in chaining and conversion.
    ///
    /// # Parameters
    ///
    /// * `id` - The ID to wrap.
    ///
    fn opt<I: Into<Self>>(id: I) -> Option<T>;
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

/// Wrapper type around `Vec<String>` to implement [`FromSql`] and [`ToSql`].
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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

/// Implement the `Id` trait for a proton id declared with [`proton_api_core::declare_proton_id`]
/// macro.
///
/// This macro should be used if the proton id does not have a local id counterpart.
#[macro_export]
macro_rules! impl_id_for_proton_id {
    ($proton_id:ident) => {
        impl $crate::datatypes::Id for $proton_id {
            async fn load<T>(
                &self,
                tether: &::stash::stash::Tether,
            ) -> Result<Option<T>, ::stash::stash::StashError>
            where
                T: ::stash::orm::Model,
            {
                T::find_first(
                    "WHERE remote_id = ?",
                    ::stash::params![self.clone()],
                    tether,
                )
                .await
            }

            fn id_field_name() -> &'static str {
                "remote_id"
            }
        }
    };
}

/// Declare a new Local id type that maps to a remote Proton Id.
///
/// A local identifier should exist for every remote/proton Id for every resource we store
/// in the database that we will create/mutate.
///
/// # Example
///
/// ```
/// use proton_api_core::declare_proton_id;
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

        impl $crate::datatypes::Id for $name {
            async fn load<T>(&self, tether: &::stash::stash::Tether) -> Result<Option<T>, ::stash::stash::StashError>
            where
                T: ::stash::orm::Model,
            {
                T::find_first("WHERE local_id = ?", ::stash::params![*self], tether).await
            }

            fn id_field_name() -> &'static str {
                "local_id"
            }
        }

        impl $crate::datatypes::IdCounterpart for $name {
            type Counterpart = $remote_id;

            async fn counterpart<T>(&self, tether: &::stash::stash::Tether) -> Result<Option<Self::Counterpart>, ::stash::stash::StashError>
            where
                T: ::stash::orm::Model,
            {
                Ok(tether
                    .query_values::<_, String>(
                        ::indoc::formatdoc!(
                            "
                            SELECT
                                remote_id AS value
                            FROM
                                {}
                            WHERE
                                local_id = ?
                            LIMIT 1
                            ",
                            T::table_name(),
                        ),
                        ::stash::params![*self],
                    )
                    .await?
                    .into_iter()
                    .next()
                    .map($remote_id::new))
            }

            async fn counterparts<T>(
                ids: Vec<Self>,
                tether: &::stash::stash::Tether,
            ) -> Result<Vec<Self::Counterpart>, ::stash::stash::StashError>
            where
                T: ::stash::orm::Model,
            {
                use ::stash::exports::ToSql;

                let placeholders = ::std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(", ");
                #[allow(trivial_casts)]
                let values = ids
                    .into_iter()
                    .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
                    .collect();
                Ok(tether
                    .query_values::<_, String>(
                        indoc::formatdoc!(
                            "
                            SELECT
                                remote_id AS id
                            FROM
                                {}
                            WHERE
                                local_id IN ({})
                            ",
                            T::table_name(),
                            placeholders,
                        ),
                        values,
                    )
                    .await?
                    .into_iter()
                    .map($remote_id::new)
                    .collect())
            }
        }

        $crate::impl_id_for_proton_id!($remote_id);

        impl $crate::datatypes::IdCounterpart for $remote_id {
            type Counterpart = $name;

            async fn counterpart<T>(&self, tether: &::stash::stash::Tether) -> Result<Option<Self::Counterpart>, ::stash::stash::StashError>
            where
                T: ::stash::orm::Model,
            {
                match tether
                    .query_value::<_, u64>(
                        ::indoc::formatdoc!(
                            "
                            SELECT
                                local_id AS value
                            FROM
                                {}
                            WHERE
                                remote_id = ?
                            LIMIT 1
                            ",
                            T::table_name(),
                        ),
                        ::stash::params![self.clone()],
                    )
                    .await
                {
                    Ok(v) => Ok(Some(v.into())),
                    Err(e) => {
                        if matches!(
                            e,
                            ::stash::stash::StashError::ExecutionError(::stash::exports::SqliteError::QueryReturnedNoRows)
                        ) {
                            Ok(None)
                        } else {
                            Err(e)
                        }
                    }
                }
            }

            async fn counterparts<T>(
                ids: Vec<Self>,
                tehter: &::stash::stash::Tether,
            ) -> Result<Vec<Self::Counterpart>, ::stash::stash::StashError>
            where
                T: ::stash::orm::Model,
            {
                use ::stash::exports::ToSql;
                let placeholders = ::std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(", ");
                #[allow(trivial_casts)]
                let values = ids
                    .into_iter()
                    .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
                    .collect();
                Ok(tehter
                    .query_values::<_, u64>(
                        ::indoc::formatdoc!(
                            "
                            SELECT
                                local_id AS value
                            FROM
                                {}
                            WHERE
                                remote_id IN ({})
                            ",
                            T::table_name(),
                            placeholders,
                        ),
                        values,
                    )
                    .await?
                    .into_iter()
                    .map(Into::into)
                    .collect())
            }
        }
    };
}

/// Local ID.
///
/// This minimal struct is simply a wrapper around a [`u64`], and is used to
/// formalise all IDs used for internal storage, and to present associated
/// functionality.
///
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LocalId(u64);

impl LocalId {
    /// Represents the internal value as an unsigned 64-bit integer.
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

impl AsRef<u64> for LocalId {
    fn as_ref(&self) -> &u64 {
        &self.0
    }
}

impl Deref for LocalId {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for LocalId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for LocalId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<LocalId> for u64 {
    fn from(id: LocalId) -> Self {
        id.0
    }
}

impl FromSql for LocalId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u64::column_result(value).map(LocalId)
    }
}

impl Id for LocalId {
    async fn load<T>(&self, tether: &Tether) -> Result<Option<T>, StashError>
    where
        T: Model,
    {
        T::find_first("WHERE local_id = ?", params![*self], tether).await
    }

    fn id_field_name() -> &'static str {
        "local_id"
    }
}

impl IdCounterpart for LocalId {
    type Counterpart = RemoteId;

    async fn counterpart<T>(&self, tether: &Tether) -> Result<Option<Self::Counterpart>, StashError>
    where
        T: Model,
    {
        Ok(tether
            .query::<_, QueryResultRemoteId>(
                formatdoc!(
                    "
                    SELECT
                        remote_id AS id
                    FROM
                        {}
                    WHERE
                        local_id = ?
                    LIMIT 1
                    ",
                    T::table_name(),
                ),
                params![*self],
            )
            .await?
            .into_iter()
            .next()
            .map(|r| r.id))
    }

    async fn counterparts<T>(
        ids: Vec<Self>,
        tether: &Tether,
    ) -> Result<Vec<Self::Counterpart>, StashError>
    where
        T: Model,
    {
        let placeholders = repeat("?").take(ids.len()).collect::<Vec<_>>().join(", ");
        #[allow(trivial_casts)]
        let values = ids
            .into_iter()
            .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
            .collect();
        Ok(tether
            .query::<_, QueryResultRemoteId>(
                formatdoc!(
                    "
                    SELECT
                        remote_id AS id
                    FROM
                        {}
                    WHERE
                        local_id IN ({})
                    ",
                    T::table_name(),
                    placeholders,
                ),
                values,
            )
            .await?
            .into_iter()
            .map(|r| r.id)
            .collect())
    }
}

impl IdOpt<Self> for LocalId {
    fn opt<I: Into<Self>>(id: I) -> Option<Self> {
        Some(id.into())
    }
}

impl ToSql for LocalId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

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

/// A query result that returns a remote ID field.
#[derive(Clone, DbRecord, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct QueryResultRemoteId {
    /// The remote ID field value.
    #[DbField]
    pub id: RemoteId,
}

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

pub use proton_api_core::RemoteId;

impl Id for RemoteId {
    async fn load<T>(&self, tether: &Tether) -> Result<Option<T>, StashError>
    where
        T: Model,
    {
        T::find_first("WHERE remote_id = ?", params![self.clone()], tether).await
    }

    fn id_field_name() -> &'static str {
        "remote_id"
    }
}

impl IdCounterpart for RemoteId {
    type Counterpart = LocalId;

    async fn counterpart<T>(&self, tether: &Tether) -> Result<Option<Self::Counterpart>, StashError>
    where
        T: Model,
    {
        match tether
            .query_value::<_, u64>(
                formatdoc!(
                    "
                    SELECT
                        local_id AS value
                    FROM
                        {}
                    WHERE
                        remote_id = ?
                    LIMIT 1
                    ",
                    T::table_name(),
                ),
                params![self.clone()],
            )
            .await
        {
            Ok(v) => Ok(Some(v.into())),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn counterparts<T>(
        ids: Vec<Self>,
        tehter: &Tether,
    ) -> Result<Vec<Self::Counterpart>, StashError>
    where
        T: Model,
    {
        let placeholders = repeat("?").take(ids.len()).collect::<Vec<_>>().join(", ");
        #[allow(trivial_casts)]
        let values = ids
            .into_iter()
            .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
            .collect();
        Ok(tehter
            .query_values::<_, u64>(
                formatdoc!(
                    "
                    SELECT
                        local_id AS value
                    FROM
                        {}
                    WHERE
                        remote_id IN ({})
                    ",
                    T::table_name(),
                    placeholders,
                ),
                values,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}

impl IdOpt<Self> for RemoteId {
    fn opt<I: Into<Self>>(id: I) -> Option<Self> {
        Some(id.into())
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SettingsFlags {
    /// TODO: Document this field.
    pub welcomed: bool,
}

impl From<ApiSettingsFlags> for SettingsFlags {
    fn from(value: ApiSettingsFlags) -> Self {
        Self {
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

/// Wrapper type around `Vec<String>` to hold the auth scope(s) of a session.
///
/// TODO: Use a `HashSet`?
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuthScopes(Vec<String>);

impl AuthScopes {
    /// Create a new [`AuthScopes`] instance from a list of [`String`]s.
    ///
    /// # Parameters
    ///
    /// * `scopes` - The scopes to wrap.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum PasswordMode {
    One = 1,
    Two = 2,
}

impl PasswordMode {
    /// Returns true if any type of additional password is active.
    #[must_use]
    pub fn want_mbp(self) -> bool {
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
    fn to_sql(&self) -> Result<ToSqlOutput, SqlError> {
        Ok(u8::from(self.to_owned()).into())
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
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// A simple wrapper around a [`u64`] to represent a timestamp.
///
/// Represents the number of seconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Create a new [`Timestamp`] at the current time.
    ///
    /// # Panics
    ///
    /// Panics if the system time is before the Unix epoch.
    #[must_use]
    pub fn now() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_secs();

        Self(now)
    }

    /// Returns the inner value as a [`u64`].
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

impl FromSql for Timestamp {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        u64::column_result(value).map(Timestamp)
    }
}

impl ToSql for Timestamp {
    fn to_sql(&self) -> Result<ToSqlOutput, SqliteError> {
        u64::to_sql(&self.0)
    }
}
