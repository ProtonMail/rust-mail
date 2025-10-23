//! Data types for Proton Core.
//!
//! This module contains the various data types used by Proton Core, i.e. those
//! that are common to all Proton applications.
//!
//! # Organisation
//!
//! The vast majority of the available data types are presented through this
//! module, and the focus is on those data types that are persistent, i.e.
//! stored in the database. In some cases there are special types with a
//! specific purpose that might be presented elsewhere. This method of
//! organisation may change over time as better patterns evolve.
//!
//! # Rust internals
//!
//! The types exposed here are carefully-prepared, lightweight facades that are
//! somewhat but not exactly analogous to the internal types used by the Proton
//! Core library. They are designed to be used by the FFI bindings, and are
//! prepared with those in mind. In this way they represent a translation layer
//! between the internal types and the FFI types, in the same way that there is
//! also a translation layer between the internal types and the Proton REST API
//! types. This gives the full ability to amend the external FFI interface as
//! necessary without affecting the internal types, and vice versa.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton internal
//! types to the exported FFI types and vice versa are provided, but not any
//! serialisation or deserialisation or other conversions. The conversions to
//! and from internal types are usually very simple and indeed in many cases can
//! be done without altering any data in memory.
//!
//! # Notable exclusions
//!
//! The following types are excluded from export via UniFFI, as they do not need
//! to be used outside of the Rust internals:
//!
//!   - [`AddressKeys`](proton_core_common::datatypes::AddressKeys)
//!   - [`UserKeys`](proton_core_common::datatypes::UserKeys)
//!
//! The following fields are excluded from represented types (in addition to
//! internal database fields):
//!
//!   - [`Address::keys`](proton_core_common::datatypes::Address::keys)
//!   - [`User::keys`](proton_core_common::datatypes::User::keys)
//!

mod account_details;
mod app_settings;
mod avatar;
mod connection_status;
pub mod contact_details;
mod contact_list;
mod issue_report;
mod timestamp;

use crate::core::resolver::Resolver;
use crate::core::resolver::ResolverImpl;

pub use self::account_details::*;
pub use self::app_settings::*;
pub use self::avatar::*;
pub use self::connection_status::*;
pub use self::contact_list::*;
pub use self::issue_report::*;
pub use self::timestamp::*;
use itertools::Itertools;
use muon::common::IntoDyn;
use muon::common::ParseEndpointErr;
use muon::env::EnvId;
use proton_core_api::session::EnvIdExt;
use proton_mail_api::services::proton::common::MessageId;
use stash::orm::Model;
use stash::stash::Tether;
use tracing::error;

use core::fmt;
use proton_core_common::datatypes::{
    AddressSignedKeyList as RealAddressSignedKeyList, AddressStatus as RealAddressStatus,
    AddressType as RealAddressType, ApiConfig as RealApiConfig, AppDetails as RealAppDetails,
    ContactSendingPreferences as RealContactSendingPreferences, DateFormat as RealDateFormat,
    Density as RealDensity, DeviceEnvironment as RealDeviceEnvironment,
    EarlyAccess as RealEarlyAccess, Email as RealEmail, FidoKey as RealFidoKey, Flags as RealFlags,
    HighSecurity as RealHighSecurity, LocalAddressId, LocalContactEmailId, LocalContactId,
    LocalLabelId, LogAuth as RealLogAuth, Password as RealPassword, Phone as RealPhone,
    ProductUsedSpace as RealProductUsedSpace, Referral as RealReferral,
    SettingsFlags as RealSettingsFlags, TfaStatus as RealTfaStatus, TimeFormat as RealTimeFormat,
    TwoFa as RealTwoFa, UserMnemonicStatus as RealUserMnemonicStatus, UserType as RealUserType,
    WeekStart as RealWeekStart,
};
use proton_core_common::models::Label as RealLabel;
use proton_core_common::models::{
    Address as RealAddress, Contact as RealContact, ContactCard as RealContactCard,
    ContactEmail as RealContactEmail, ModelIdExtension, Role as RealRole, User as RealUser,
    UserSettings as RealUserSettings,
};
use proton_core_common::utils::MapVec as _;
use proton_crypto_account::contacts::ContactCardType as RealCardType;
use proton_mail_common::AppError;
use proton_mail_common::datatypes::{LocalAttachmentId, LocalConversationId, LocalMessageId};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::sync::Arc;
use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};
//  ENUMS
//==============================================================================

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum AddressStatus {
    /// TODO: Document this field.
    Disabled = 0,

    /// TODO: Document this field.
    Enabled = 1,

    /// TODO: Document this field.
    Deleting = 2,
}

impl From<AddressStatus> for RealAddressStatus {
    fn from(status: AddressStatus) -> Self {
        match status {
            AddressStatus::Disabled => Self::Disabled,
            AddressStatus::Enabled => Self::Enabled,
            AddressStatus::Deleting => Self::Deleting,
        }
    }
}

impl From<RealAddressStatus> for AddressStatus {
    fn from(status: RealAddressStatus) -> Self {
        match status {
            RealAddressStatus::Disabled => Self::Disabled,
            RealAddressStatus::Enabled => Self::Enabled,
            RealAddressStatus::Deleting => Self::Deleting,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
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

impl From<AddressType> for RealAddressType {
    fn from(address_type: AddressType) -> Self {
        match address_type {
            AddressType::Original => Self::Original,
            AddressType::Alias => Self::Alias,
            AddressType::Custom => Self::Custom,
            AddressType::Premium => Self::Premium,
            AddressType::External => Self::External,
        }
    }
}

impl From<RealAddressType> for AddressType {
    fn from(address_type: RealAddressType) -> Self {
        match address_type {
            RealAddressType::Original => Self::Original,
            RealAddressType::Alias => Self::Alias,
            RealAddressType::Custom => Self::Custom,
            RealAddressType::Premium => Self::Premium,
            RealAddressType::External => Self::External,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
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

impl From<CardType> for RealCardType {
    fn from(card_type: CardType) -> Self {
        match card_type {
            CardType::ClearText => Self::ClearText,
            CardType::Encrypted => Self::Encrypted,
            CardType::Signed => Self::Signed,
            CardType::EncryptedAndSigned => Self::EncryptedAndSigned,
        }
    }
}

impl From<RealCardType> for CardType {
    fn from(card_type: RealCardType) -> Self {
        match card_type {
            RealCardType::ClearText => Self::ClearText,
            RealCardType::Encrypted => Self::Encrypted,
            RealCardType::Signed => Self::Signed,
            RealCardType::EncryptedAndSigned => Self::EncryptedAndSigned,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ContactSendingPreferences {
    /// TODO: Document this variant.
    Custom = 0,

    /// TODO: Document this variant.
    Default = 1,
}

impl From<ContactSendingPreferences> for RealContactSendingPreferences {
    fn from(preference: ContactSendingPreferences) -> Self {
        match preference {
            ContactSendingPreferences::Custom => Self::Custom,
            ContactSendingPreferences::Default => Self::Default,
        }
    }
}

impl From<RealContactSendingPreferences> for ContactSendingPreferences {
    fn from(preference: RealContactSendingPreferences) -> Self {
        match preference {
            RealContactSendingPreferences::Custom => Self::Custom,
            RealContactSendingPreferences::Default => Self::Default,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
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

impl From<DateFormat> for RealDateFormat {
    fn from(date_format: DateFormat) -> Self {
        match date_format {
            DateFormat::Default => Self::Default,
            DateFormat::DdMmYyyy => Self::DdMmYyyy,
            DateFormat::MmDdYyyy => Self::MmDdYyyy,
            DateFormat::YyyyMmDd => Self::YyyyMmDd,
        }
    }
}

impl From<RealDateFormat> for DateFormat {
    fn from(date_format: RealDateFormat) -> Self {
        match date_format {
            RealDateFormat::Default => Self::Default,
            RealDateFormat::DdMmYyyy => Self::DdMmYyyy,
            RealDateFormat::MmDdYyyy => Self::MmDdYyyy,
            RealDateFormat::YyyyMmDd => Self::YyyyMmDd,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum Density {
    /// TODO: Document this variant.
    Comfortable = 0,

    /// TODO: Document this variant.
    Compact = 1,
}

impl From<Density> for RealDensity {
    fn from(density: Density) -> Self {
        match density {
            Density::Comfortable => Self::Comfortable,
            Density::Compact => Self::Compact,
        }
    }
}

impl From<RealDensity> for Density {
    fn from(density: RealDensity) -> Self {
        match density {
            RealDensity::Comfortable => Self::Comfortable,
            RealDensity::Compact => Self::Compact,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum EarlyAccess {
    /// TODO: Document this variant.
    Regular = 0,

    /// TODO: Document this variant.
    Beta = 1,
}

impl From<EarlyAccess> for RealEarlyAccess {
    fn from(early_access: EarlyAccess) -> Self {
        match early_access {
            EarlyAccess::Regular => Self::Regular,
            EarlyAccess::Beta => Self::Beta,
        }
    }
}

impl From<RealEarlyAccess> for EarlyAccess {
    fn from(early_access: RealEarlyAccess) -> Self {
        match early_access {
            RealEarlyAccess::Regular => Self::Regular,
            RealEarlyAccess::Beta => Self::Beta,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum LogAuth {
    /// TODO: Document this variant.
    Disabled = 0,

    /// TODO: Document this variant.
    Basic = 1,

    /// TODO: Document this variant.
    Advanced = 2,
}

impl From<LogAuth> for RealLogAuth {
    fn from(log_auth: LogAuth) -> Self {
        match log_auth {
            LogAuth::Disabled => Self::Disabled,
            LogAuth::Basic => Self::Basic,
            LogAuth::Advanced => Self::Advanced,
        }
    }
}

impl From<RealLogAuth> for LogAuth {
    fn from(log_auth: RealLogAuth) -> Self {
        match log_auth {
            RealLogAuth::Disabled => Self::Disabled,
            RealLogAuth::Basic => Self::Basic,
            RealLogAuth::Advanced => Self::Advanced,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, UniffiEnum)]
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

impl From<TfaStatus> for RealTfaStatus {
    fn from(tfa_status: TfaStatus) -> Self {
        match tfa_status {
            TfaStatus::None => Self::None,
            TfaStatus::Totp => Self::Totp,
            TfaStatus::Fido2 => Self::Fido2,
            TfaStatus::TotpOrFido2 => Self::TotpOrFido2,
        }
    }
}

impl From<RealTfaStatus> for TfaStatus {
    fn from(tfa_status: RealTfaStatus) -> Self {
        match tfa_status {
            RealTfaStatus::None => Self::None,
            RealTfaStatus::Totp => Self::Totp,
            RealTfaStatus::Fido2 => Self::Fido2,
            RealTfaStatus::TotpOrFido2 => Self::TotpOrFido2,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum TimeFormat {
    /// TODO: Document this variant.
    Default = 0,

    /// TODO: Document this variant.
    H24 = 1,

    /// TODO: Document this variant.
    H12 = 2,
}

impl From<TimeFormat> for RealTimeFormat {
    fn from(time_format: TimeFormat) -> Self {
        match time_format {
            TimeFormat::Default => Self::Default,
            TimeFormat::H24 => Self::H24,
            TimeFormat::H12 => Self::H12,
        }
    }
}

impl From<RealTimeFormat> for TimeFormat {
    fn from(time_format: RealTimeFormat) -> Self {
        match time_format {
            RealTimeFormat::Default => Self::Default,
            RealTimeFormat::H24 => Self::H24,
            RealTimeFormat::H12 => Self::H12,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
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

impl From<UserMnemonicStatus> for RealUserMnemonicStatus {
    fn from(mnemonic_status: UserMnemonicStatus) -> Self {
        match mnemonic_status {
            UserMnemonicStatus::Disabled => Self::Disabled,
            UserMnemonicStatus::EnabledButNotSet => Self::EnabledButNotSet,
            UserMnemonicStatus::EnabledNeedsReactivation => Self::EnabledNeedsReactivation,
            UserMnemonicStatus::EnabledAndSet => Self::EnabledAndSet,
            UserMnemonicStatus::Unknown => Self::Unknown,
        }
    }
}

impl From<RealUserMnemonicStatus> for UserMnemonicStatus {
    fn from(mnemonic_status: RealUserMnemonicStatus) -> Self {
        match mnemonic_status {
            RealUserMnemonicStatus::Disabled => Self::Disabled,
            RealUserMnemonicStatus::EnabledButNotSet => Self::EnabledButNotSet,
            RealUserMnemonicStatus::EnabledNeedsReactivation => Self::EnabledNeedsReactivation,
            RealUserMnemonicStatus::EnabledAndSet => Self::EnabledAndSet,
            RealUserMnemonicStatus::Unknown => Self::Unknown,
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum UserType {
    /// TODO: Document this variant.
    Proton,

    /// TODO: Document this variant.
    Managed,

    /// TODO: Document this variant.
    External,

    /// Credentialless user
    CredentialLess,

    /// TODO: Document this variant.
    Unknown(u8),
}

impl From<UserType> for RealUserType {
    fn from(user_type: UserType) -> Self {
        match user_type {
            UserType::Proton => Self::Proton,
            UserType::Managed => Self::Managed,
            UserType::External => Self::External,
            UserType::CredentialLess => Self::CredentialLess,
            UserType::Unknown(v) => Self::Unknown(v),
        }
    }
}

impl From<RealUserType> for UserType {
    fn from(user_type: RealUserType) -> Self {
        match user_type {
            RealUserType::Proton => Self::Proton,
            RealUserType::Managed => Self::Managed,
            RealUserType::External => Self::External,
            RealUserType::CredentialLess => Self::CredentialLess,
            RealUserType::Unknown(v) => Self::Unknown(v),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum Role {
    None,
    Member,
    Admin,
    Unknown(u32),
}

impl From<Role> for RealRole {
    fn from(role: Role) -> Self {
        match role {
            Role::None => Self::None,
            Role::Member => Self::Member,
            Role::Admin => Self::Admin,
            Role::Unknown(v) => Self::Unknown(v),
        }
    }
}

impl From<RealRole> for Role {
    fn from(role: RealRole) -> Self {
        match role {
            RealRole::None => Self::None,
            RealRole::Member => Self::Member,
            RealRole::Admin => Self::Admin,
            RealRole::Unknown(v) => Self::Unknown(v),
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
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

impl From<WeekStart> for RealWeekStart {
    fn from(week_start: WeekStart) -> Self {
        match week_start {
            WeekStart::Default => Self::Default,
            WeekStart::Monday => Self::Monday,
            WeekStart::Saturday => Self::Saturday,
            WeekStart::Sunday => Self::Sunday,
        }
    }
}

impl From<RealWeekStart> for WeekStart {
    fn from(week_start: RealWeekStart) -> Self {
        match week_start {
            RealWeekStart::Default => Self::Default,
            RealWeekStart::Monday => Self::Monday,
            RealWeekStart::Saturday => Self::Saturday,
            RealWeekStart::Sunday => Self::Sunday,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, UniffiEnum)]
pub enum NonDefaultWeekStart {
    Monday = 1,
    Saturday = 6,
    Sunday = 7,
}

impl From<NonDefaultWeekStart> for RealWeekStart {
    fn from(week_start: NonDefaultWeekStart) -> Self {
        match week_start {
            NonDefaultWeekStart::Monday => Self::Monday,
            NonDefaultWeekStart::Saturday => Self::Saturday,
            NonDefaultWeekStart::Sunday => Self::Sunday,
        }
    }
}

/// In which environment are we going to register the device
/// for the push notification.
///
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
pub enum DeviceEnvironment {
    Google,
    AppleProd,
    AppleBeta,
    AppleProdET,
    AppleDevET,
    AppleDev,
}

impl From<RealDeviceEnvironment> for DeviceEnvironment {
    fn from(value: RealDeviceEnvironment) -> Self {
        match value {
            RealDeviceEnvironment::Google => Self::Google,
            RealDeviceEnvironment::AppleProd => Self::AppleProd,
            RealDeviceEnvironment::AppleBeta => Self::AppleBeta,
            RealDeviceEnvironment::AppleProdET => Self::AppleProdET,
            RealDeviceEnvironment::AppleDevET => Self::AppleDevET,
            RealDeviceEnvironment::AppleDev => Self::AppleDev,
        }
    }
}

impl From<DeviceEnvironment> for RealDeviceEnvironment {
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

/// A set of tokens.
///
/// This type represents the tokens held by the client.
/// Depending on the state of the client's auth session, it can be either
/// a single refresh token (which must be refreshed before use) or an access
/// token (and associated refresh token and scopes).
#[derive(uniffi::Enum)]
pub enum MigrationTokens {
    /// A single refresh token.
    ///
    /// This token must be refreshed before use;
    /// once refreshed, it becomes an access token.
    Refresh {
        /// The refresh token's value.
        refresh_token: String,
    },

    /// An access token.
    ///
    /// This token can be used to make authenticated requests to the Proton API.
    /// It is associated with a refresh token, used to get a new access token
    /// when the current one expires, and a set of scopes, which define the
    /// permissions granted by the token.
    Access {
        /// The access token.
        access_token: String,

        /// The refresh token.
        refresh_token: String,
    },
}

//  STRUCTS
//==============================================================================

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub id: Id,

    /// TODO: Document this field.
    pub address_type: AddressType,

    /// TODO: Document this field.
    pub catch_all: bool,

    /// TODO: Document this field.
    pub display_name: String,

    /// TODO: Document this field.
    pub display_order: u32,

    /// TODO: Document this field.
    pub domain_id: Option<String>,

    /// TODO: Document this field.
    pub email: String,

    /// TODO: Document this field.
    pub proton_mx: bool,

    /// TODO: Document this field.
    pub receive: bool,

    /// TODO: Document this field.
    pub send: bool,

    /// TODO: Document this field.
    pub signature: String,

    /// TODO: Document this field.
    pub signed_key_list: AddressSignedKeyList,

    /// TODO: Document this field.
    pub status: AddressStatus,
}

impl From<RealAddress> for Address {
    fn from(address: RealAddress) -> Self {
        Self {
            id: address.id().into(),
            address_type: address.address_type.into(),
            catch_all: address.catch_all,
            display_name: address.display_name,
            display_order: address.display_order,
            domain_id: address.domain_id,
            email: address.email,
            proton_mx: address.proton_mx,
            receive: address.receive,
            send: address.send,
            signature: address.signature,
            signed_key_list: address.signed_key_list.into(),
            status: address.status.into(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
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

impl From<AddressSignedKeyList> for RealAddressSignedKeyList {
    fn from(signed_key_list: AddressSignedKeyList) -> Self {
        Self {
            data: signed_key_list.data,
            expected_min_epoch_id: signed_key_list.expected_min_epoch_id,
            max_epoch_id: signed_key_list.max_epoch_id,
            min_epoch_id: signed_key_list.min_epoch_id,
            obsolescence_token: signed_key_list.obsolescence_token,
            revision: signed_key_list.revision,
            signature: signed_key_list.signature,
        }
    }
}

impl From<RealAddressSignedKeyList> for AddressSignedKeyList {
    fn from(signed_key_list: RealAddressSignedKeyList) -> Self {
        Self {
            data: signed_key_list.data,
            expected_min_epoch_id: signed_key_list.expected_min_epoch_id,
            max_epoch_id: signed_key_list.max_epoch_id,
            min_epoch_id: signed_key_list.min_epoch_id,
            obsolescence_token: signed_key_list.obsolescence_token,
            revision: signed_key_list.revision,
            signature: signed_key_list.signature,
        }
    }
}

/// An environment identifier.
///
/// This enum represents the different environments that can be used by the
/// API client. The environments are used to determine the base URL for the
/// API requests and the TLS pins to use.
#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum ApiEnvId {
    /// The production API environment.
    ///
    /// This environment represents the production Proton API, used by default.
    /// Clients configured with this environment will connect to `https://<app>.proton.me/`,
    /// with the exact domain depending on the app version.
    Prod,

    /// The standard atlas environment.
    ///
    /// Clients configured with this environment will connect to `https://proton.black/api`.
    Atlas,

    /// A named atlas environment.
    ///
    /// Clients configured with this environment will connect to `https://<name>.proton.black/api`.
    Scientist(String),

    /// A specific environment specified by its URL.
    ///
    /// Clients configured with this environment will connect to the specified URL,
    /// which must be a valid URL with a scheme, host, and if necessary, a port.
    ///
    /// This is useful for testing but MUST NOT be used in production.
    /// Ideally, this would be protected by compile-time feature flags.
    ///
    /// TODO: Protect this with a compile-time feature flag.
    Custom(String),
}

/// The configuration for the Proton API service.
#[derive(Clone, Debug, UniffiRecord)]
pub struct ApiConfig {
    /// TODO: Document this field.
    pub user_agent: String,

    /// Env to connect to.
    pub env_id: ApiEnvId,

    /// Proxy to use.
    pub proxy: Option<String>,

    /// A resolver to use.
    pub resolver: Option<Arc<dyn Resolver>>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            user_agent: String::from("NoClient/0.1.0"),
            env_id: ApiEnvId::Prod,
            proxy: None,
            resolver: None,
        }
    }
}

impl ApiConfig {
    pub fn into_real_api_config(
        self,
        details: AppDetails,
    ) -> Result<RealApiConfig, ParseEndpointErr> {
        let env_id = match self.env_id {
            ApiEnvId::Prod => EnvId::new_prod(),
            ApiEnvId::Atlas => EnvId::new_atlas(),
            ApiEnvId::Scientist(name) => EnvId::new_atlas_name(name),
            ApiEnvId::Custom(server) => EnvId::new_custom_url(server)?,
        };

        Ok(RealApiConfig {
            app_details: RealAppDetails::from(details),
            user_agent: Some(self.user_agent),
            proxy: self.proxy,
            resolver: self.resolver.map(|r| ResolverImpl::new(r).into_dyn()),
            env_id,
        })
    }
}

#[derive(Clone, UniffiRecord)]
pub struct AppDetails {
    /// Example: "ios"
    pub platform: String,
    /// Example: "mail"
    pub product: String,
    /// Example: "1.0.0"
    pub version: String,
}

impl From<AppDetails> for RealAppDetails {
    fn from(details: AppDetails) -> Self {
        Self {
            platform: details.platform,
            product: details.product,
            version: details.version,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Contact {
    /// TODO: Document this field.
    pub cards: Vec<ContactCard>,

    /// TODO: Document this field.
    pub contact_emails: Vec<ContactEmail>,

    /// TODO: Document this field.
    pub create_time: u64,

    /// TODO: Document this field.
    pub label_ids: Vec<Id>,

    /// TODO: Document this field.
    pub modify_time: u64,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

impl Contact {
    /// Converts a [`RealContact`] into a [`Contact`].
    pub async fn try_from_real(value: RealContact, tether: &Tether) -> Result<Self, AppError> {
        let mut contact_emails = Vec::with_capacity(value.contact_emails.len());
        for email in &value.contact_emails {
            contact_emails.push(ContactEmail::try_from_real(email.clone(), tether).await?);
        }

        Ok(Self {
            cards: value.cards.map_vec(),
            contact_emails,
            create_time: value.create_time,
            label_ids: RealLabel::remote_ids_counterpart(
                value.label_ids.into_inner().into_iter().collect(),
                tether,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
            modify_time: value.modify_time,
            name: value.name,
            size: value.size,
        })
    }
}

/// Represents a contact card.
///
/// Contact cards contain information encoded as a v-card. Cards can be
/// encrypted or signed with the user keys.
///
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactCard {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    pub id: Id,

    /// TODO: Document this field.
    pub card_type: CardType,

    /// TODO: Document this field.
    pub data: String,

    /// TODO: Document this field.
    pub signature: Option<String>,
}

impl From<RealContactCard> for ContactCard {
    fn from(card: RealContactCard) -> Self {
        Self {
            id: card.id().into(),
            card_type: card.card_type.into(),
            data: card.data,
            signature: card.signature,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactEmail {
    /// TODO: Document this field.
    pub canonical_email: String,

    /// TODO: Document this field.
    pub contact_type: Vec<String>,

    /// TODO: Document this field.
    pub defaults: ContactSendingPreferences,

    /// TODO: Document this field.
    pub display_order: u32,

    /// TODO: Document this field.
    pub email: String,

    /// TODO: Document this field.
    pub is_proton: bool,

    /// TODO: Document this field.
    pub label_ids: Vec<Id>,

    /// TODO: Document this field.
    pub last_used_time: UnixTimestamp,

    /// TODO: Document this field.
    pub name: String,
}

impl ContactEmail {
    /// Converts a [`RealContactEmail`] into a [`ContactEmail`].
    pub async fn try_from_real(value: RealContactEmail, tether: &Tether) -> Result<Self, AppError> {
        Ok(Self {
            canonical_email: value.canonical_email.into_clear_text_string(),
            contact_type: value.contact_type.deref().clone(),
            defaults: value.defaults.into(),
            display_order: value.display_order,
            email: value.email.into_clear_text_string(),
            is_proton: value.is_proton,
            label_ids: RealLabel::remote_ids_counterpart(value.label_ids.into_inner(), tether)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            last_used_time: value.last_used_time.into(),
            name: value.name,
        })
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
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

impl From<Email> for RealEmail {
    fn from(email: Email) -> Self {
        Self {
            notify: email.notify,
            reset: email.reset,
            status: email.status,
            value: email.value,
        }
    }
}

impl From<RealEmail> for Email {
    fn from(email: RealEmail) -> Self {
        Self {
            notify: email.notify,
            reset: email.reset,
            status: email.status,
            value: email.value,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct FidoKey {
    /// TODO: Document this field.
    pub attestation_format: String,

    /// TODO: Document this field.
    pub credential_id: Vec<i32>,

    /// TODO: Document this field.
    pub name: String,
}

impl From<FidoKey> for RealFidoKey {
    fn from(key: FidoKey) -> Self {
        Self {
            attestation_format: key.attestation_format,
            credential_id: key.credential_id,
            name: key.name,
        }
    }
}

impl From<RealFidoKey> for FidoKey {
    fn from(key: RealFidoKey) -> Self {
        Self {
            attestation_format: key.attestation_format,
            credential_id: key.credential_id,
            name: key.name,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
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

impl From<Flags> for RealFlags {
    fn from(flags: Flags) -> Self {
        Self {
            has_temporary_password: flags.has_temporary_password,
            no_login: flags.no_login,
            no_proton_address: flags.no_proton_address,
            onboard_checklist_storage_granted: flags.onboard_checklist_storage_granted,
            protected: flags.protected,
            recovery_attempt: flags.recovery_attempt,
            sso: flags.sso,
            test_account: flags.test_account,
        }
    }
}

impl From<RealFlags> for Flags {
    fn from(flags: RealFlags) -> Self {
        Self {
            has_temporary_password: flags.has_temporary_password,
            no_login: flags.no_login,
            no_proton_address: flags.no_proton_address,
            onboard_checklist_storage_granted: flags.onboard_checklist_storage_granted,
            protected: flags.protected,
            recovery_attempt: flags.recovery_attempt,
            sso: flags.sso,
            test_account: flags.test_account,
        }
    }
}

/// Local ID.
///
/// This minimal struct is simply a wrapper around a [`u64`], and is used to
/// formalise all IDs used internally for saving to the database.
///
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, UniffiRecord)]
pub struct Id {
    value: u64,
}

impl Id {
    /// Represents the internal value as an unsigned 64-bit integer.
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.value
    }
}

impl Deref for Id {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<Id> for u64 {
    fn from(id: Id) -> Self {
        id.value
    }
}

impl From<u64> for Id {
    fn from(id: u64) -> Self {
        Self { value: id }
    }
}

macro_rules! impl_into_id {
    ($name:ident) => {
        impl From<Id> for $name {
            fn from(id: Id) -> Self {
                Self::from(id.value)
            }
        }

        impl From<$name> for Id {
            fn from(id: $name) -> Self {
                Self { value: id.as_u64() }
            }
        }
    };
}

//TODO: Improve uniffi local_id types without causing mayhem.
impl_into_id!(LocalAddressId);
impl_into_id!(LocalLabelId);
impl_into_id!(LocalContactId);
impl_into_id!(LocalContactEmailId);
impl_into_id!(LocalAttachmentId);
impl_into_id!(LocalMessageId);
impl_into_id!(LocalConversationId);

/// Remote ID
///
/// This data type should be used as a last resort.
/// If possible, use [`Id`] instead.
///
/// This struct is a simple wrapper around [`String`] and
/// is used to formalise all IDs used by our API.
///
#[derive(Clone, Debug, Eq, Hash, PartialEq, UniffiRecord)]
pub struct RemoteId {
    value: String,
}

macro_rules! impl_into_remote_id {
    ($name:ident) => {
        impl From<RemoteId> for $name {
            fn from(id: RemoteId) -> Self {
                Self::from(id.value)
            }
        }

        impl From<$name> for RemoteId {
            fn from(id: $name) -> Self {
                Self {
                    value: id.into_inner(),
                }
            }
        }
    };
}

impl_into_remote_id!(MessageId);

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct HighSecurity {
    /// TODO: Document this field.
    pub eligible: bool,

    /// TODO: Document this field.
    pub value: bool,
}

impl From<HighSecurity> for RealHighSecurity {
    fn from(high_security: HighSecurity) -> Self {
        Self {
            eligible: high_security.eligible,
            value: high_security.value,
        }
    }
}

impl From<RealHighSecurity> for HighSecurity {
    fn from(high_security: RealHighSecurity) -> Self {
        Self {
            eligible: high_security.eligible,
            value: high_security.value,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct Password {
    /// TODO: Document this field.
    pub mode: u32,

    /// TODO: Document this field.
    pub expiration_time: Option<u64>,
}

impl From<Password> for RealPassword {
    fn from(password: Password) -> Self {
        Self {
            mode: password.mode,
            expiration_time: password.expiration_time,
        }
    }
}

impl From<RealPassword> for Password {
    fn from(password: RealPassword) -> Self {
        Self {
            mode: password.mode,
            expiration_time: password.expiration_time,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
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

impl From<Phone> for RealPhone {
    fn from(phone: Phone) -> Self {
        Self {
            notify: phone.notify,
            reset: phone.reset,
            status: phone.status,
            value: phone.value,
        }
    }
}

impl From<RealPhone> for Phone {
    fn from(phone: RealPhone) -> Self {
        Self {
            notify: phone.notify,
            reset: phone.reset,
            status: phone.status,
            value: phone.value,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
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

impl From<ProductUsedSpace> for RealProductUsedSpace {
    fn from(product_used_space: ProductUsedSpace) -> Self {
        Self {
            calendar: product_used_space.calendar,
            contact: product_used_space.contact,
            drive: product_used_space.drive,
            mail: product_used_space.mail,
            pass: product_used_space.pass,
        }
    }
}

impl From<RealProductUsedSpace> for ProductUsedSpace {
    fn from(product_used_space: RealProductUsedSpace) -> Self {
        Self {
            calendar: product_used_space.calendar,
            contact: product_used_space.contact,
            drive: product_used_space.drive,
            mail: product_used_space.mail,
            pass: product_used_space.pass,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct Referral {
    /// TODO: Document this field.
    pub eligible: bool,

    /// TODO: Document this field.
    pub link: String,
}

impl From<Referral> for RealReferral {
    fn from(referral: Referral) -> Self {
        Self {
            eligible: referral.eligible,
            link: referral.link,
        }
    }
}

impl From<RealReferral> for Referral {
    fn from(referral: RealReferral) -> Self {
        Self {
            eligible: referral.eligible,
            link: referral.link,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct SettingsFlags {
    /// TODO: Document this field.
    pub welcomed: bool,
    /// `EasyDeviceMigration` (QR Login) opt out. The user can choose to disable the feature.
    pub edm_opt_out: bool,
}

impl From<SettingsFlags> for RealSettingsFlags {
    fn from(flags: SettingsFlags) -> Self {
        Self {
            welcomed: flags.welcomed,
            edm_opt_out: flags.edm_opt_out,
        }
    }
}

impl From<RealSettingsFlags> for SettingsFlags {
    fn from(flags: RealSettingsFlags) -> Self {
        Self {
            welcomed: flags.welcomed,
            edm_opt_out: flags.edm_opt_out,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
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

impl From<TwoFa> for RealTwoFa {
    fn from(two_fa: TwoFa) -> Self {
        Self {
            allowed: two_fa.allowed.into(),
            enabled: two_fa.enabled.into(),
            expiration_time: two_fa.expiration_time,
            registered_keys: two_fa
                .registered_keys
                .into_iter()
                .map(RealFidoKey::from)
                .collect(),
        }
    }
}

impl From<RealTwoFa> for TwoFa {
    fn from(two_fa: RealTwoFa) -> Self {
        Self {
            allowed: two_fa.allowed.into(),
            enabled: two_fa.enabled.into(),
            expiration_time: two_fa.expiration_time,
            registered_keys: two_fa
                .registered_keys
                .into_iter()
                .map(FidoKey::from)
                .collect(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct User {
    /// TODO: Document this field.
    pub create_time: UnixTimestamp,

    /// TODO: Document this field.
    pub credit: i64,

    /// TODO: Document this field.
    pub currency: String,

    /// TODO: Document this field.
    pub delinquent: u32,

    /// TODO: Document this field.
    pub display_name: Option<String>,

    /// TODO: Document this field.
    pub email: String,

    /// TODO: Document this field.
    pub flags: Flags,

    /// TODO: Document this field.
    pub max_space: i64,

    /// TODO: Document this field.
    pub max_upload: i64,

    /// TODO: Document this field.
    pub mnemonic_status: UserMnemonicStatus,

    /// TODO: Document this field.
    pub private: bool,

    /// TODO: Document this field.
    pub name: Option<String>,

    /// TODO: Document this field.
    pub product_used_space: ProductUsedSpace,

    /// TODO: Document this field.
    pub role: Role,

    /// TODO: Document this field.
    pub services: u32,

    /// TODO: Document this field.
    pub subscribed: u32,

    /// TODO: Document this field.
    pub to_migrate: bool,

    /// TODO: Document this field.
    pub used_space: i64,

    /// TODO: Document this field.
    pub user_type: UserType,
}

impl From<RealUser> for User {
    fn from(user: RealUser) -> Self {
        Self {
            create_time: user.create_time.into(),
            credit: user.credit,
            currency: user.currency,
            delinquent: user.delinquent as u32,
            display_name: user.display_name,
            email: user.email,
            flags: user.flags.into(),
            max_space: user.max_space,
            max_upload: user.max_upload,
            mnemonic_status: user.mnemonic_status.into(),
            private: user.private,
            name: user.name,
            product_used_space: user.product_used_space.into(),
            role: user.role.into(),
            services: user.services,
            subscribed: user.subscribed.bits(),
            to_migrate: user.to_migrate,
            used_space: user.used_space,
            user_type: user.user_type.into(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct UserSettings {
    /// TODO: Document this field.
    pub crash_reports: bool,

    /// TODO: Document this field.
    pub date_format: DateFormat,

    /// TODO: Document this field.
    pub density: Density,

    /// TODO: Document this field.
    pub device_recovery: bool,

    /// TODO: Document this field.
    pub early_access: bool,

    /// TODO: Document this field.
    pub email: Email,

    /// TODO: Document this field.
    pub flags: SettingsFlags,

    /// TODO: Document this field.
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
    pub session_account_recovery: bool,

    /// TODO: Document this field.
    pub telemetry: bool,

    /// TODO: Document this field.
    pub time_format: TimeFormat,

    /// TODO: Document this field.
    pub two_factor_auth: TwoFa,

    /// TODO: Document this field.
    pub week_start: WeekStart,

    /// TODO: Document this field.
    pub welcome: bool,
}

impl From<RealUserSettings> for UserSettings {
    fn from(settings: RealUserSettings) -> Self {
        Self {
            crash_reports: settings.crash_reports,
            date_format: settings.date_format.into(),
            density: settings.density.into(),
            device_recovery: settings.device_recovery,
            early_access: settings.early_access,
            email: settings.email.into(),
            flags: settings.flags.into(),
            hide_side_panel: settings.hide_side_panel,
            high_security: settings.high_security.into(),
            invoice_text: settings.invoice_text,
            locale: settings.locale,
            log_auth: settings.log_auth.into(),
            news: settings.news,
            password: settings.password.into(),
            phone: settings.phone.into(),
            referral: settings.referral.map(Referral::from),
            session_account_recovery: settings.session_account_recovery,
            telemetry: settings.telemetry,
            time_format: settings.time_format.into(),
            two_factor_auth: settings.two_factor_auth.into(),
            week_start: settings.week_start.into(),
            welcome: settings.welcome,
        }
    }
}

use proton_core_api::services::proton::AppleRecurringReceiptDetails as RealAppleRecurringReceiptDetails;
use proton_core_api::services::proton::GetPaymentsPlansOptions as RealGetPaymentsPlansOptions;
use proton_core_api::services::proton::GoogleRecurringReceiptDetails as RealGoogleRecurringReceiptDetails;
use proton_core_api::services::proton::Location as RealLocation;
use proton_core_api::services::proton::NewSubscription as RealNewSubscription;
use proton_core_api::services::proton::NewSubscriptionValues as RealNewSubscriptionValues;
use proton_core_api::services::proton::PaymentMethods as RealPaymentMethods;
use proton_core_api::services::proton::PaymentReceipt as RealPaymentReceipt;
use proton_core_api::services::proton::PaymentVendor as RealPaymentVendor;
use proton_core_api::services::proton::PaymentVendorState as RealPaymentVendorState;
use proton_core_api::services::proton::Plan as RealPlan;
use proton_core_api::services::proton::PlanDecoration as RealPlanDecoration;
use proton_core_api::services::proton::PlanEntitlement as RealPlanEntitlement;
use proton_core_api::services::proton::PlanInstance as RealPlanInstance;
use proton_core_api::services::proton::PlanPrice as RealPlanPrice;
use proton_core_api::services::proton::PlanType as RealPlanType;
use proton_core_api::services::proton::PlanVendor as RealPlanVendor;
use proton_core_api::services::proton::PlanVendorName as RealPlanVendorName;
use proton_core_api::services::proton::Subscription as RealSubscription;
use proton_core_api::services::proton::SubscriptionId;

/// Represents a single payment plan from the Proton API.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Plan {
    pub id: String,
    pub description: String,
    pub name: Option<String>,
    pub title: String,
    pub state: PlanState,
    pub r#type: PlanType,
    pub features: PlanFeatures,
    pub services: PlanServices,
    pub offers: Vec<String>,
    pub layout: String,
    pub instances: Vec<PlanInstance>,
    pub entitlements: Vec<PlanEntitlement>,
    pub decorations: Vec<PlanDecoration>,
}

impl From<RealPlan> for Plan {
    fn from(plan: RealPlan) -> Self {
        let instances = plan.instances.into_iter().map(PlanInstance::from).collect();

        let entitlements = plan
            .entitlements
            .into_iter()
            .map(PlanEntitlement::from)
            .collect();

        let decorations = plan
            .decorations
            .into_iter()
            .map(PlanDecoration::from)
            .collect();

        let offers = plan
            .offers
            .into_iter()
            .map(|offer| serde_json::to_string(&offer))
            .try_collect()
            .inspect_err(|e| error!("failed to serialize offer: {e:?}"))
            .unwrap();

        Self {
            id: plan.id.into_inner(),
            description: plan.description,
            name: plan.name,
            title: plan.title,
            state: plan.state,
            r#type: plan.r#type.into(),
            features: plan.features,
            services: plan.services,
            layout: plan.layout,

            offers,
            instances,
            entitlements,
            decorations,
        }
    }
}

/// A plan state.
pub type PlanState = u8;

/// A plan type.
#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum PlanType {
    SubPlan = 0,
    PrimaryPlan = 1,
}

impl From<RealPlanType> for PlanType {
    fn from(plan_type: RealPlanType) -> Self {
        match plan_type {
            RealPlanType::SubPlan => Self::SubPlan,
            RealPlanType::PrimaryPlan => Self::PrimaryPlan,
        }
    }
}

/// A plan features bitmask.
pub type PlanFeatures = u8;

/// A plan services bitmask.
pub type PlanServices = u8;

/// Represents a plan instance.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct PlanInstance {
    pub cycle: u8,
    pub description: String,
    pub period_end: u64,
    pub price: Vec<PlanPrice>,
    pub vendors: HashMap<PlanVendorName, PlanVendor>,
}

impl From<RealPlanInstance> for PlanInstance {
    fn from(instance: RealPlanInstance) -> Self {
        let price = instance.price.into_iter().map(PlanPrice::from).collect();

        let vendors = instance
            .vendors
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        Self {
            cycle: instance.cycle,
            description: instance.description,
            period_end: instance.period_end,

            price,
            vendors,
        }
    }
}

/// Represents a plan price.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct PlanPrice {
    pub id: String,
    pub currency: String,
    pub current: u64,
}

impl From<RealPlanPrice> for PlanPrice {
    fn from(price: RealPlanPrice) -> Self {
        Self {
            id: price.id,
            currency: price.currency,
            current: price.current,
        }
    }
}

/// Represents a plan vendor's name.
#[derive(Clone, Debug, Eq, PartialEq, Hash, UniffiEnum)]
pub enum PlanVendorName {
    Google,
    Apple,
}

impl From<RealPlanVendorName> for PlanVendorName {
    fn from(name: RealPlanVendorName) -> Self {
        match name {
            RealPlanVendorName::Google => Self::Google,
            RealPlanVendorName::Apple => Self::Apple,
        }
    }
}

/// Represents data for a plan vendor.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct PlanVendor {
    pub product_id: String,
    pub customer_id: Option<String>,
}

#[allow(clippy::redundant_closure_for_method_calls)]
impl From<RealPlanVendor> for PlanVendor {
    fn from(vendor: RealPlanVendor) -> Self {
        Self {
            product_id: vendor.product_id.into_inner(),
            customer_id: vendor.customer_id.map(|id| id.into_inner()),
        }
    }
}

/// Represents a plan entitlement.
#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum PlanEntitlement {
    Description {
        text: String,
        icon_name: String,
        hint: Option<String>,
    },
    Progress {
        text: String,
        title: Option<String>,
        min: u64,
        max: u64,
        current: u64,
        icon_name: Option<String>,
    },
}

impl From<RealPlanEntitlement> for PlanEntitlement {
    fn from(entitlement: RealPlanEntitlement) -> Self {
        match entitlement {
            RealPlanEntitlement::Description {
                text,
                icon_name,
                hint,
            } => Self::Description {
                text,
                icon_name,
                hint,
            },
            RealPlanEntitlement::Progress {
                text,
                title,
                min,
                max,
                current,
                icon_name,
            } => Self::Progress {
                text,
                title,
                min,
                max,
                current,
                icon_name,
            },
        }
    }
}

/// Represents a plan decoration.
#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum PlanDecoration {
    Starred {
        icon_name: String,
    },
    Badge {
        text: String,
        anchor: String,
        plan_id: String,
    },
}

impl From<RealPlanDecoration> for PlanDecoration {
    fn from(decoration: RealPlanDecoration) -> Self {
        match decoration {
            RealPlanDecoration::Starred { icon_name } => Self::Starred { icon_name },

            RealPlanDecoration::Badge {
                text,
                anchor,
                plan_id,
            } => Self::Badge {
                text,
                anchor,
                plan_id: plan_id.into_inner(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum PaymentReceipt {
    AppleRecurring {
        details: AppleRecurringReceiptDetails,
    },
    Google {
        details: GoogleRecurringReceiptDetails,
    },
}

impl From<PaymentReceipt> for RealPaymentReceipt {
    fn from(receipt: PaymentReceipt) -> Self {
        match receipt {
            PaymentReceipt::AppleRecurring { details } => Self::AppleRecurring {
                details: details.into(),
            },
            PaymentReceipt::Google { details } => Self::Google {
                details: details.into(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AppleRecurringReceiptDetails {
    pub transaction_id: String,
    pub product_id: String,
    pub bundle_id: String,
    pub receipt: String,
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct GoogleRecurringReceiptDetails {
    pub order_id: String,
    pub customer_id: String,
    pub product_id: String,
    pub package_name: String,
    pub token: String,
}

impl From<AppleRecurringReceiptDetails> for RealAppleRecurringReceiptDetails {
    fn from(details: AppleRecurringReceiptDetails) -> Self {
        Self {
            transaction_id: details.transaction_id.into(),
            product_id: details.product_id.into(),
            bundle_id: details.bundle_id.into(),
            receipt: details.receipt,
        }
    }
}

impl From<GoogleRecurringReceiptDetails> for RealGoogleRecurringReceiptDetails {
    fn from(details: GoogleRecurringReceiptDetails) -> Self {
        Self {
            order_id: details.order_id.into(),
            customer_id: details.customer_id.into(),
            product_id: details.product_id.into(),
            package_name: details.package_name.into(),
            token: details.token,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Subscription {
    pub id: Option<String>,
    pub name: Option<String>,

    pub title: String,
    pub description: String,

    pub cycle: Option<u8>,
    pub cycle_description: Option<String>,

    pub currency: Option<String>,
    pub offer: Option<String>,

    pub amount: Option<u64>,
    pub renew_amount: Option<u64>,

    pub discount: Option<i64>,
    pub renew_discount: Option<i64>,

    pub period_start: Option<u64>,
    pub period_end: Option<u64>,
    pub create_time: Option<u64>,
    pub coupon_code: Option<String>,

    pub renew: Option<u8>,
    pub external: Option<u8>,

    pub entitlements: Vec<PlanEntitlement>,
    pub decorations: Vec<PlanDecoration>,
}

impl From<RealSubscription> for Subscription {
    fn from(subscription: RealSubscription) -> Self {
        let id = subscription.id.map(SubscriptionId::into_inner);

        let entitlements = subscription
            .entitlements
            .into_iter()
            .map(From::from)
            .collect();

        let decorations = subscription
            .decorations
            .into_iter()
            .map(From::from)
            .collect();

        Self {
            id,
            name: subscription.name,
            title: subscription.title,
            description: subscription.description,
            cycle: subscription.cycle,
            cycle_description: subscription.cycle_description,
            currency: subscription.currency,
            offer: subscription.offer,
            amount: subscription.amount,
            renew_amount: subscription.renew_amount,
            discount: subscription.discount,
            renew_discount: subscription.renew_discount,
            period_start: subscription.period_start,
            period_end: subscription.period_end,
            create_time: subscription.create_time,
            coupon_code: subscription.coupon_code,
            renew: subscription.renew,
            external: subscription.external,
            entitlements,
            decorations,
        }
    }
}

/// Subscription details
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct NewSubscription {
    pub cycle: u8,
    pub currency: Option<String>,
    pub currency_id: Option<i32>,
    pub plans: Option<HashMap<String, i32>>,
    pub plan_ids: Option<Vec<i32>>,
    pub codes: Option<Vec<String>>,
    pub coupon_code: Option<String>,
    pub gift_code: Option<String>,
}

impl From<NewSubscription> for RealNewSubscription {
    fn from(subscription: NewSubscription) -> Self {
        Self {
            cycle: subscription.cycle,
            currency: subscription.currency,
            currency_id: subscription.currency_id,
            plans: subscription.plans,
            plan_ids: subscription.plan_ids,
            codes: subscription.codes,
            coupon_code: subscription.coupon_code,
            gift_code: subscription.gift_code,
        }
    }
}

/// New subscription values
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct NewSubscriptionValues {
    pub amount: Option<u64>,
    pub payments: Option<Vec<String>>,
    pub payment_token: Option<String>,
}

impl From<NewSubscriptionValues> for RealNewSubscriptionValues {
    fn from(values: NewSubscriptionValues) -> Self {
        Self {
            amount: values.amount,
            payments: values.payments,
            payment_token: values.payment_token,
        }
    }
}

/// Options for getting payments plans.
#[derive(uniffi::Record)]
pub struct GetPaymentsPlansOptions {
    pub currency: Option<String>,
    pub vendor: Option<String>,
    pub state: Option<u8>,
    pub timestamp: Option<u64>,
    pub fallback: Option<bool>,
}

impl From<GetPaymentsPlansOptions> for RealGetPaymentsPlansOptions {
    fn from(filter: GetPaymentsPlansOptions) -> Self {
        RealGetPaymentsPlansOptions {
            currency: filter.currency,
            vendor: filter.vendor,
            state: filter.state,
            timestamp: filter.timestamp,
            fallback: filter.fallback,
        }
    }
}

impl From<RealGetPaymentsPlansOptions> for GetPaymentsPlansOptions {
    fn from(filter: RealGetPaymentsPlansOptions) -> Self {
        GetPaymentsPlansOptions {
            currency: filter.currency,
            vendor: filter.vendor,
            state: filter.state,
            timestamp: filter.timestamp,
            fallback: filter.fallback,
        }
    }
}

/// Payment plans available to the user.
#[derive(uniffi::Record)]
pub struct PaymentsPlans {
    /// The list of plans available to the user.
    pub plans: Vec<Plan>,

    /// What cycle to display by default
    pub default_cycle: u8,
}

/// A payment token.
#[derive(uniffi::Record)]
pub struct PaymentToken {
    pub token: String,
    pub status: u64,
}

/// Current subscriptions.
#[derive(uniffi::Record)]
pub struct Subscriptions {
    pub current: Vec<Subscription>,
    pub upcoming: Vec<Subscription>,
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct PaymentsStatus {
    /// Geolocation for this request.
    pub location: Location,
    /// Status of supported vendors.
    pub payment_methods: PaymentMethods,
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Location {
    pub country_code: Option<String>,
    pub state: Option<String>,
    pub zip_code: Option<String>,
}

impl From<RealLocation> for Location {
    fn from(location: RealLocation) -> Self {
        Self {
            country_code: location.country_code,
            state: location.state,
            zip_code: location.zip_code,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct PaymentMethods {
    pub bitcoin: PaymentVendor,
    pub card: PaymentVendor,
    pub in_app: PaymentVendor,
    pub paypal: PaymentVendor,
}

impl From<RealPaymentMethods> for PaymentMethods {
    fn from(methods: RealPaymentMethods) -> Self {
        Self {
            bitcoin: methods.bitcoin.into(),
            card: methods.card.into(),
            in_app: methods.in_app.into(),
            paypal: methods.paypal.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct PaymentVendor {
    /// Whether the vendor is enabled/disabled for this user & location.
    pub state: PaymentVendorState,
    /// Reason when a vendor is disabled.
    pub reason: Option<String>,
}

impl From<RealPaymentVendor> for PaymentVendor {
    fn from(vendor: RealPaymentVendor) -> Self {
        Self {
            state: vendor.state.into(),
            reason: vendor.reason,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, UniffiEnum)]
pub enum PaymentVendorState {
    /// Vendor is disabled.
    Disabled = 0,
    /// Vendor is enabled.
    Enabled = 1,
}

impl From<RealPaymentVendorState> for PaymentVendorState {
    fn from(state: RealPaymentVendorState) -> Self {
        match state {
            RealPaymentVendorState::Disabled => PaymentVendorState::Disabled,
            RealPaymentVendorState::Enabled => PaymentVendorState::Enabled,
        }
    }
}
