//! Common types used by the Proton API.
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

//  STRUCTS
//==============================================================================

/// If the `sql` feature is enabled this marker will contain extra trait boundaries.
#[cfg(feature = "sql")]
pub trait ProtonIdSqlMarker: ::stash::exports::ToSql + ::stash::exports::FromSql {}

#[cfg(not(feature = "sql"))]
/// If the `sql` feature is enabled this marker will contain extra trait boundaries.
pub trait ProtonIdSqlMarker {}

/// Marker trait assigned to each id that was declared with [`declare_proton_id`].
pub trait ProtonIdMarker:
    std::ops::Deref<Target = str>
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

/// Declare a new unique type for a Proton String Identifier.
///
/// # Example
///
/// ```
/// use proton_api_core::declare_proton_id;
/// declare_proton_id!(pub MyProtonId);
///
/// let id = MyProtonId::from("my-actual-proton-id");
/// ```
#[macro_export]
macro_rules! declare_proton_id {
    (
        $(#[$($attrss:tt)*])*
        $visibility:vis $name:ident
    ) => {
        $(#[$($attrss)*])*
        #[derive(Clone, Debug, serde::Deserialize, Eq, Hash, PartialEq, serde::Serialize)]
        $visibility struct $ name(String);

        impl $name {
            #[doc ="Create a new [`"]
            #[doc =stringify!($name)]
            #[doc ="`] from a [`String`]."]
            ///
            /// # Parameters
            ///
            /// * `id` - The ID to wrap.
            ///
            #[must_use]
            pub fn new(id: String) -> Self {
                Self(id)
            }

            #[doc = "Convert the [`"]
            #[doc = stringify!($name)]
            #[doc = "`] into the inner [`String`]."]
            #[must_use]
            pub fn into_inner(self) -> String {
                self.0
            }

            /// Get a reference to the inner [`String`]
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name{
            fn from(id: String) -> Self {
                Self(id)
            }
        }

        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.to_owned())
            }
        }

        impl ::std::ops::Deref for $name {
            type Target = str;

            fn deref(&self) -> &Self::Target {
                self.0.as_str()
            }
        }

        #[cfg(feature = "sql")]
        impl ::stash::exports::ToSql for $name {
            fn to_sql(&self) -> Result<::stash::exports::ToSqlOutput<'_>, ::stash::exports::SqliteError> {
                self.as_str().to_sql()
            }
        }

        #[cfg(feature = "sql")]
        impl ::stash::exports::FromSql for $name {
            fn column_result(value: stash::exports::ValueRef<'_>) -> ::stash::exports::FromSqlResult<Self> {
                String::column_result(value).map(Self)
            }
        }

        impl $crate::services::proton::common::ProtonIdSqlMarker for $name {}

        impl $crate::services::proton::common::ProtonIdMarker for $name {}
    }
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
    /// Represents the Id of an active network Session.
    pub AuthId
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
