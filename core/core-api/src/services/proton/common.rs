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
use serde_json::Value as JsonValue;
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::serde_as;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Deref;

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

/// A currency (enum of string, can be EUR, USD or CHF).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum Currency {
    EUR,
    USD,
    CHF,
}

//  STRUCTS
//==============================================================================

/// Represents a single payment plan from the Proton API.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Plan {
    #[serde(rename = "ID")]
    pub id: PlanId,
    pub description: String,
    pub name: Option<String>,
    pub title: String,
    pub state: PlanState,
    pub r#type: PlanType,
    pub features: PlanFeatures,
    pub services: PlanServices,
    pub offers: Vec<JsonValue>,
    pub layout: String,
    pub instances: Vec<PlanInstance>,
    pub entitlements: Vec<PlanEntitlement>,
    pub decorations: Vec<PlanDecoration>,
}

/// A plan state.
pub type PlanState = u8;

/// A plan type.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize_repr))]
#[repr(u8)]
pub enum PlanType {
    /// A sub-plan (add-on).
    SubPlan = 0,

    /// A primary plan.
    PrimaryPlan = 1,
}

/// A plan features bitmask.
pub type PlanFeatures = u8;

/// A plan services bitmask.
pub type PlanServices = u8;

/// Represents a plan instance.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PlanInstance {
    pub cycle: PlanCycle,
    pub description: String,
    pub period_end: u64,
    pub price: Vec<PlanPrice>,
    pub vendors: HashMap<PlanVendorName, PlanVendor>,
}

/// A plan cycle, in months.
#[derive(Clone, Copy, Debug, Serialize_repr, Deserialize_repr, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PlanCycle {
    OneMonth = 1,
    OneYear = 12,
    TwoYears = 24,
}

/// Represents a plan price (object, with currency, current and default price).
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PlanPrice {
    #[serde(rename = "ID")]
    pub id: String,
    pub currency: Currency,
    pub current: u64,
}

/// Represents a plan vendor's name.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
pub enum PlanVendorName {
    Google,
    Apple,
}

/// Represents data for a plan vendor.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PlanVendor {
    #[serde(rename = "ProductID")]
    pub product_id: ProductId,

    #[serde(rename = "CustomerID")]
    pub customer_id: Option<CustomerId>,
}

/// Represents a plan entitlement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(tag = "Type")]
pub enum PlanEntitlement {
    #[serde(rename_all = "PascalCase", rename = "description")]
    Description {
        text: String,
        icon_name: String,
        hint: Option<String>,
    },

    #[serde(rename_all = "PascalCase", rename = "progress")]
    Progress {
        text: String,
        min: u64,
        max: u64,
        current: u64,
        icon_name: String,
    },
}

/// Represents a plan decoration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(tag = "Type")]
pub enum PlanDecoration {
    #[serde(rename_all = "PascalCase", rename = "starred")]
    Starred { icon_name: String },

    #[serde(rename_all = "PascalCase", rename = "badge")]
    Badge {
        text: String,
        anchor: String,

        #[serde(rename = "PlanID")]
        plan_id: PlanId,
    },
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

//  MACROS
//==============================================================================

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
    /// Represents the Id of a plan.
    pub PlanId
}

declare_proton_id! {
    /// Represents the Id of a product.
    pub ProductId
}

declare_proton_id! {
    /// Represents the Id of a customer.
    pub CustomerId
}

declare_proton_id! {
    /// Represents the Id of a bundle.
    pub BundleId
}

declare_proton_id! {
    /// Represents the Id of a payment transaction.
    pub TransactionId
}

declare_proton_id! {
    /// Represents the Id of a customer.
    pub PaymentMethodId
}
