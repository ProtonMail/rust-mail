//! Common types used by the Proton API.
//!
//! This module provides child data types that are used for both requests and
//! responses, and are not specific to any one endpoint.
//!
//! The structs in this module should NOT have any business logic or other
//! functionality.
//!
#[cfg(feature = "mocks")]
use serde::Serialize;
#[cfg(feature = "mocks")]
use serde_repr::Serialize_repr;

use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_repr::Deserialize_repr;
use serde_with::{BoolFromInt, serde_as};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use crate::declare_proton_id;
use crate::services::proton::common::deserialize_bool_from_string;

//  STRUCTS
//==============================================================================

/// Represents a single payment plan from the Proton API.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
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
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
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
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PlanInstance {
    pub cycle: u8,
    pub description: String,
    pub period_end: u64,
    pub price: Vec<PlanPrice>,
    pub vendors: HashMap<PlanVendorName, PlanVendor>,
}

/// Represents a plan price (object, with currency, current and default price).
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PlanPrice {
    #[serde(rename = "ID")]
    pub id: String,
    pub currency: String,
    pub current: u64,
}

/// Represents a plan vendor's name.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
pub enum PlanVendorName {
    Google,
    Apple,
}

/// Represents data for a plan vendor.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PlanVendor {
    #[serde(rename = "ProductID")]
    pub product_id: ProductId,

    #[serde(rename = "CustomerID")]
    pub customer_id: Option<CustomerId>,
}

/// Represents a plan entitlement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
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
        title: Option<String>,
        min: u64,
        max: u64,
        current: u64,
        icon_name: Option<String>,
    },
}

/// Represents a plan decoration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
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

/// Represents an active subscription.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Subscription {
    #[serde(rename = "ID")]
    pub id: Option<SubscriptionId>,
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
    pub billing_platform: Option<u8>,

    pub entitlements: Vec<PlanEntitlement>,
    pub decorations: Vec<PlanDecoration>,
}

/// User location.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct Location {
    pub country_code: Option<String>,
    pub state: Option<String>,
    pub zip_code: Option<String>,
}

/// Supported vendors.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PaymentMethods {
    pub bitcoin: PaymentVendor,
    pub card: PaymentVendor,
    pub in_app: PaymentVendor,
    pub paypal: PaymentVendor,
}

/// Status of a vendor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PaymentVendor {
    /// Whether the vendor is enabled/disabled for this user & location.
    pub state: PaymentVendorState,
    /// Reason when a vendor is disabled.
    pub reason: Option<String>,
}

/// Vendor state.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize_repr))]
#[repr(u8)]
pub enum PaymentVendorState {
    /// Vendor is disabled.
    Disabled = 0,
    /// Vendor is enabled.
    Enabled = 1,
}

declare_proton_id! {
    pub PlanId
}
declare_proton_id! {
    pub ProductId
}
declare_proton_id! {
    pub CustomerId
}
declare_proton_id! {
    pub BundleId
}
declare_proton_id! {
    pub PackageNameId
}
declare_proton_id! {
    pub TransactionId
}
declare_proton_id! {
    pub OrderId
}
declare_proton_id! {
    pub PaymentMethodId
}
declare_proton_id! {
    pub SubscriptionId
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PaymentMethod {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Type")]
    pub payment_type: String,
    #[serde_as(as = "BoolFromInt")]
    pub autopay: bool,
    #[serde_as(as = "BoolFromInt")]
    pub external: bool,
    pub order: i32,
    pub details: PaymentMethodDetails,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(untagged)]
pub enum PaymentMethodDetails {
    Card(PaymentMethodCardDetails),
    Paypal(PaymentMethodPaypalDetails),
    DirectDebit(PaymentMethodDirectDebitDetails),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PaymentMethodCardDetails {
    pub last4: String,
    pub brand: String,
    pub exp_month: String,
    pub exp_year: String,
    pub name: Option<String>,
    pub country: Option<String>,
    #[serde(rename = "ZIP")]
    pub zip: Option<String>,
    #[serde(
        rename = "ThreeDSSupport",
        deserialize_with = "deserialize_bool_from_string"
    )]
    pub three_ds_support: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PaymentMethodPaypalDetails {
    #[serde(rename = "BillingAgreementID")]
    pub billing_agreement_id: String,
    #[serde(rename = "PayerID")]
    pub payer_id: Option<String>,
    pub payer: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PaymentMethodDirectDebitDetails {
    pub account_name: String,
    pub country: String,
    pub last4: String,
}
