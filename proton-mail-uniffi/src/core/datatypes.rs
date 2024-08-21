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

use core::fmt;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::Config as RealApiConfig;
use proton_api_core::{DEFAULT_APP_VERSION, DEFAULT_CLIENT, DEFAULT_HOST_URL};
use proton_core_common::datatypes::{
    AddressSignedKeyList as RealAddressSignedKeyList, AddressStatus as RealAddressStatus,
    AddressType as RealAddressType, CardType as RealCardType,
    ContactSendingPreferences as RealContactSendingPreferences, ContactTypes as RealContactTypes,
    DateFormat as RealDateFormat, Density as RealDensity, EarlyAccess as RealEarlyAccess,
    Email as RealEmail, FidoKey as RealFidoKey, Flags as RealFlags,
    HighSecurity as RealHighSecurity, LabelId as RealLabelId, Labels as RealLabels,
    LocalId as RealLocalId, LogAuth as RealLogAuth, Password as RealPassword, Phone as RealPhone,
    ProductUsedSpace as RealProductUsedSpace, Referral as RealReferral, RemoteId as RealRemoteId,
    SettingsFlags as RealSettingsFlags, TfaStatus as RealTfaStatus, TimeFormat as RealTimeFormat,
    TwoFa as RealTwoFa, UserMnemonicStatus as RealUserMnemonicStatus, UserType as RealUserType,
    WeekStart as RealWeekStart,
};
use proton_core_common::models::{
    Address as RealAddress, Contact as RealContact, ContactCard as RealContactCard,
    ContactEmail as RealContactEmail, User as RealUser, UserSettings as RealUserSettings,
};
use smart_default::SmartDefault;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
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
#[repr(u8)]
pub enum UserType {
    /// TODO: Document this variant.
    Proton = 1,

    /// TODO: Document this variant.
    Managed = 2,

    /// TODO: Document this variant.
    External = 3,
}

impl From<UserType> for RealUserType {
    fn from(user_type: UserType) -> Self {
        match user_type {
            UserType::Proton => Self::Proton,
            UserType::Managed => Self::Managed,
            UserType::External => Self::External,
        }
    }
}

impl From<RealUserType> for UserType {
    fn from(user_type: RealUserType) -> Self {
        match user_type {
            RealUserType::Proton => Self::Proton,
            RealUserType::Managed => Self::Managed,
            RealUserType::External => Self::External,
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
            id: address.local_id.unwrap().into(),
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

/// The configuration for the Proton API service.
#[derive(Clone, Debug, Eq, PartialEq, SmartDefault, UniffiRecord)]
pub struct ApiConfig {
    /// TODO: Document this field.
    pub allow_http: bool,

    /// TODO: Document this field.
    #[default(DEFAULT_APP_VERSION.to_owned())]
    pub app_version: String,

    /// The base URL for the external service.
    #[default(DEFAULT_HOST_URL.to_owned())]
    pub base_url: String,

    /// TODO: Document this field.
    pub skip_srp_proof_validation: bool,

    /// TODO: Document this field.
    #[default(DEFAULT_CLIENT.to_owned())]
    pub user_agent: String,
}

impl From<ApiConfig> for RealApiConfig {
    fn from(config: ApiConfig) -> Self {
        Self {
            allow_http: config.allow_http,
            app_version: config.app_version,
            base_url: config.base_url,
            skip_srp_proof_validation: config.skip_srp_proof_validation,
            user_agent: config.user_agent,
        }
    }
}

impl From<RealApiConfig> for ApiConfig {
    fn from(config: RealApiConfig) -> Self {
        Self {
            allow_http: config.allow_http,
            app_version: config.app_version,
            base_url: config.base_url,
            skip_srp_proof_validation: config.skip_srp_proof_validation,
            user_agent: config.user_agent,
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
    pub label_ids: Labels,

    /// TODO: Document this field.
    pub modify_time: u64,

    /// TODO: Document this field.
    pub name: String,

    /// TODO: Document this field.
    pub size: u64,
}

impl From<RealContact> for Contact {
    fn from(contact: RealContact) -> Self {
        Self {
            cards: contact.cards.into_iter().map(ContactCard::from).collect(),
            contact_emails: contact
                .contact_emails
                .into_iter()
                .map(ContactEmail::from)
                .collect(),
            create_time: contact.create_time,
            label_ids: contact.label_ids.into(),
            modify_time: contact.modify_time,
            name: contact.name,
            size: contact.size,
        }
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
            id: card.local_id.unwrap().into(),
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
    pub contact_type: ContactTypes,

    /// TODO: Document this field.
    pub defaults: ContactSendingPreferences,

    /// TODO: Document this field.
    pub display_order: u32,

    /// TODO: Document this field.
    pub email: String,

    /// TODO: Document this field.
    pub is_proton: bool,

    /// TODO: Document this field.
    pub label_ids: Labels,

    /// TODO: Document this field.
    pub last_used_time: u64,

    /// TODO: Document this field.
    pub name: String,
}

impl From<RealContactEmail> for ContactEmail {
    fn from(email: RealContactEmail) -> Self {
        Self {
            canonical_email: email.canonical_email,
            contact_type: email.contact_type.into(),
            defaults: email.defaults.into(),
            display_order: email.display_order,
            email: email.email,
            is_proton: email.is_proton,
            label_ids: email.label_ids.into(),
            last_used_time: email.last_used_time,
            name: email.name,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactTypes {
    value: Vec<String>,
}

impl ContactTypes {
    /// Create a new [`ContactTypes`] instance from a list of [`String`]s.
    ///
    /// # Parameters
    ///
    /// * `types` - The types to wrap.
    ///
    #[must_use]
    pub fn new(types: Vec<String>) -> Self {
        Self { value: types }
    }

    /// Convert the [`ContactTypes`] into the inner [`Vec`].
    #[must_use]
    pub fn into_inner(self) -> Vec<String> {
        self.value
    }
}

impl Deref for ContactTypes {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl From<ContactTypes> for RealContactTypes {
    fn from(contact_types: ContactTypes) -> Self {
        Self::new(contact_types.into_inner())
    }
}

impl From<RealContactTypes> for ContactTypes {
    fn from(contact_types: RealContactTypes) -> Self {
        Self {
            value: contact_types.into_inner(),
        }
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

impl From<Id> for RealLocalId {
    fn from(id: Id) -> Self {
        Self::from(id.value)
    }
}

impl From<RealLocalId> for Id {
    fn from(id: RealLocalId) -> Self {
        Self { value: id.as_u64() }
    }
}

/// Wrapper type around `RemoteId` to implement label-specific functionality.
#[derive(Clone, Debug, Eq, Hash, PartialEq, UniffiRecord)]
pub struct LabelId {
    value: RemoteId,
}

impl LabelId {
    /// Create a new [`LabelId`] instance from a [`String`].
    ///
    /// # Parameters
    ///
    /// * `id` - The ID to wrap.
    ///
    #[must_use]
    pub fn new(id: String) -> Self {
        Self {
            value: RemoteId::new(id),
        }
    }

    /// Convert the [`LabelId`] into the inner [`RemoteId`].
    #[must_use]
    pub fn into_inner(self) -> RemoteId {
        self.value
    }
}

impl Deref for LabelId {
    type Target = RemoteId;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl Display for LabelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<LabelId> for RealLabelId {
    fn from(label_id: LabelId) -> Self {
        RealLabelId::from(RealRemoteId::from(label_id.into_inner()))
    }
}

impl From<RealLabelId> for LabelId {
    fn from(label_id: RealLabelId) -> Self {
        Self {
            value: label_id.into_inner().into(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Labels {
    value: Vec<LabelId>,
}

impl Labels {
    /// Create a new [`Labels`] instance from a list of [`LabelId`]s.
    ///
    /// # Parameters
    ///
    /// * `ids` - The IDs to wrap.
    ///
    #[must_use]
    pub fn new(ids: Vec<LabelId>) -> Self {
        Self { value: ids }
    }

    /// Convert the [`Labels`] into the inner [`Vec`].
    #[must_use]
    pub fn into_inner(self) -> Vec<LabelId> {
        self.value
    }
}

impl Deref for Labels {
    type Target = Vec<LabelId>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl From<Labels> for RealLabels {
    fn from(labels: Labels) -> Self {
        Self::new(
            labels
                .into_inner()
                .into_iter()
                .map(RealLabelId::from)
                .collect(),
        )
    }
}

impl From<RealLabels> for Labels {
    fn from(labels: RealLabels) -> Self {
        Self {
            value: labels.into_inner().into_iter().map(LabelId::from).collect(),
        }
    }
}

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

/// Remote ID.
///
/// This minimal struct is simply a wrapper around a [`String`], and is used to
/// formalise all IDs used by the Proton API.
///
#[derive(Clone, Debug, Eq, Hash, PartialEq, UniffiRecord)]
pub struct RemoteId {
    value: String,
}

impl RemoteId {
    /// Create a new [`RemoteId`] from a [`String`].
    ///
    /// # Parameters
    ///
    /// * `id` - The ID to wrap.
    ///
    #[must_use]
    pub fn new(id: String) -> Self {
        Self { value: id }
    }

    /// Convert the [`RemoteId`] into the inner [`String`].
    #[must_use]
    pub fn into_inner(self) -> String {
        self.value
    }
}

impl Deref for RemoteId {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl Display for RemoteId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<ApiRemoteId> for RemoteId {
    fn from(value: ApiRemoteId) -> Self {
        Self {
            value: value.into_inner(),
        }
    }
}

impl From<RemoteId> for ApiRemoteId {
    fn from(value: RemoteId) -> Self {
        Self::new(value.into_inner())
    }
}

impl From<RemoteId> for RealRemoteId {
    fn from(remote_id: RemoteId) -> Self {
        Self::new(remote_id.into_inner())
    }
}

impl From<RealRemoteId> for RemoteId {
    fn from(remote_id: RealRemoteId) -> Self {
        Self {
            value: remote_id.into_inner(),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct SettingsFlags {
    /// TODO: Document this field.
    pub in_app_promos_hidden: bool,

    /// TODO: Document this field.
    pub welcomed: bool,
}

impl From<SettingsFlags> for RealSettingsFlags {
    fn from(flags: SettingsFlags) -> Self {
        Self {
            in_app_promos_hidden: flags.in_app_promos_hidden,
            welcomed: flags.welcomed,
        }
    }
}

impl From<RealSettingsFlags> for SettingsFlags {
    fn from(flags: RealSettingsFlags) -> Self {
        Self {
            in_app_promos_hidden: flags.in_app_promos_hidden,
            welcomed: flags.welcomed,
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
    pub create_time: u64,

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
    pub private: u32,

    /// TODO: Document this field.
    pub name: Option<String>,

    /// TODO: Document this field.
    pub product_used_space: ProductUsedSpace,

    /// TODO: Document this field.
    pub role: u32,

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
            create_time: user.create_time,
            credit: user.credit,
            currency: user.currency,
            delinquent: user.delinquent,
            display_name: user.display_name,
            email: user.email,
            flags: user.flags.into(),
            max_space: user.max_space,
            max_upload: user.max_upload,
            mnemonic_status: user.mnemonic_status.into(),
            private: user.private,
            name: user.name,
            product_used_space: user.product_used_space.into(),
            role: user.role,
            services: user.services,
            subscribed: user.subscribed,
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
