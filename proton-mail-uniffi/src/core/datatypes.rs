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

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum ContactSendingPreferences {
    /// TODO: Document this variant.
    Custom = 0,

    /// TODO: Document this variant.
    Default = 1,
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

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum Density {
    /// TODO: Document this variant.
    Comfortable = 0,

    /// TODO: Document this variant.
    Compact = 1,
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

//  STRUCTS
//==============================================================================

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<RemoteId>,

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
    pub keys: AddressKeys,

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressKeys(pub RealAddressKeys);

impl Deref for AddressKeys {
    type Target = RealAddressKeys;

    fn deref(&self) -> &Self::Target {
        &self.0
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

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct Contact {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<RemoteId>,

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

    /// TODO: Document this field.
    pub uid: RemoteId,
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
    pub local_id: Option<u64>,

    /// TODO: Document this field.
    pub remote_contact_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub card_type: CardType,

    /// TODO: Document this field.
    pub data: String,

    /// TODO: Document this field.
    pub signature: Option<String>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct ContactEmail {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    pub remote_contact_id: Option<RemoteId>,

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

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct HighSecurity {
    /// TODO: Document this field.
    pub eligible: bool,

    /// TODO: Document this field.
    pub value: bool,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct Password {
    /// TODO: Document this field.
    pub mode: u32,

    /// TODO: Document this field.
    pub expiration_time: Option<u64>,
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

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct Referral {
    /// TODO: Document this field.
    pub eligible: bool,

    /// TODO: Document this field.
    pub link: String,
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

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Eq, PartialEq, UniffiRecord)]
pub struct SettingsFlags {
    /// TODO: Document this field.
    pub in_app_promos_hidden: bool,

    /// TODO: Document this field.
    pub welcomed: bool,
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

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct User {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<RemoteId>,

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
    pub keys: UserKeys,

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserKeys(pub RealUserKeys);

impl Deref for UserKeys {
    type Target = RealUserKeys;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
#[allow(clippy::struct_excessive_bools)]
pub struct UserSettings {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    pub remote_id: Option<RemoteId>,

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
