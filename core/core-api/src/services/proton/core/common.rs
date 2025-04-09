//! Common types used by the Proton Core API.
//!
//! This module provides child data types that are used for both requests and
//! responses, and are not specific to any one endpoint.
//!
//! The structs in this module should NOT have any business logic or other
//! functionality.
//!

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Deref;

use crate::declare_proton_id;

//  ENUMS
//==============================================================================

/// Human verification type returned by the API.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[repr(u8)]
#[serde(rename_all = "lowercase")]
pub enum HumanVerificationType {
    /// User needs to solve a Captcha, use [`crate::captcha_get`] to retrieve the token, solve in a web
    /// browser/view and retrieve the token posted via an `HVCaptchaMessage`.
    Captcha,

    /// User needs to verify via a token send via an email. Note: Request for this
    /// verification is not yet implemented.
    Email,

    /// User needs to verify via a token send via sms. Note: Request for this verification is not
    /// yet inmplemented.
    Sms,
}

impl HumanVerificationType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Captcha => "captcha",
            Self::Email => "email",
            Self::Sms => "sms",
        }
    }
}

/// The theme being used in Images Logo.
#[derive(Clone, Copy, Debug, Serialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LightOrDarkMode {
    /// Light mode
    Light,

    /// Dark mode
    Dark,
}

/// Represents which kind of label we are dealing with
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq, Serialize_repr)]
#[repr(u8)]
pub enum LabelType {
    /// TODO: Document this variant.
    Label = 1,

    /// TODO: Document this variant.
    ContactGroup = 2,

    /// TODO: Document this variant.
    Folder = 3,

    /// TODO: Document this variant.
    System = 4,
}

/// In which environment are we going to register the device
/// for the push notification.
///
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq, Serialize_repr)]
#[repr(u8)]
pub enum DeviceEnvironment {
    Google = 4,
    AppleProd = 6,
    AppleBeta = 7,
    AppleProdET = 14,
    AppleDevET = 15,
    AppleDev = 16,
}

//  TRAITS
//==============================================================================

/// If the `sql` feature is enabled this marker will contain extra trait boundaries.
#[cfg(feature = "sql")]
pub trait ProtonIdSqlMarker: ::stash::exports::ToSql + ::stash::exports::FromSql {}

#[cfg(not(feature = "sql"))]
/// If the `sql` feature is enabled this marker will contain extra trait boundaries.
pub trait ProtonIdSqlMarker {}

/// Marker trait assigned to each id that was declared with [`declare_proton_id`].
pub trait ProtonIdMarker:
    Deref<Target = str>
    + Clone
    + Debug
    + DeserializeOwned
    + Eq
    + Hash
    + PartialEq
    + ProtonIdSqlMarker
    + Serialize
    + Sync
    + Send
{
}

declare_proton_id! {
    /// Represents the Id of the user.
    pub UserId
}

declare_proton_id! {
    /// Represents the Id of a User Address.
    pub AddressId
}

declare_proton_id! {
    /// Represents the Id of an active API Session.
    pub SessionId
}

declare_proton_id! {
    /// Represents the Id of a Contact.
    pub ContactId
}

declare_proton_id! {
    /// Represents the email Id of a Contact.
    pub ContactEmailId
}

declare_proton_id! {
    /// Represents the UID of a Contact.
    pub ContactUID
}

declare_proton_id! {
    /// Represents the Id of an Event.
    pub EventId
}

declare_proton_id! {
    /// Represents the Id of a Label.
    pub LabelId
}

declare_proton_id! {
    /// Represents the Id of a crypto salt.
    pub SaltId
}

declare_proton_id! {
    /// Represents the Id of an incoming default
    pub IncomingDefaultId
}
