//! Request child data structures for the Proton API.
//!
//! This module provides child data types that are used by the request
//! structures when sending requests to the Proton API.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint request
//! definitions, and NOT have any business logic or other functionality.
//!
//! Structs in this module should only implement [`Serialize`], and should not
//! implement [`Deserialize`](serde::Deserialize). If anything in this module
//! implements [`Deserialize`](serde::Deserialize), it is a sign that a mistake
//! has been made.
//!
//! Any types that used by both requests and responses should be defined in the
//! [`common`](crate::services::proton::common) module.
//!

use crate::services::proton::prelude::*;
use serde::Serialize;
use std::collections::HashMap;

//  STRUCTS
//==============================================================================

/// Human verification data required for login.
#[derive(Clone, Debug)]
pub struct HumanVerificationData {
    /// Type of human verification where the code originated from.
    pub hv_type: HumanVerificationType,

    /// Result of the human verification request.
    pub token: String,
}

/// Payment receipt for creating a payment token.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "Type")]
pub enum PaymentReceipt {
    #[serde(rename_all = "PascalCase")]
    AppleRecurring {
        details: AppleRecurringReceiptDetails,
    },
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct AppleRecurringReceiptDetails {
    #[serde(rename = "TransactionID")]
    pub transaction_id: TransactionId,

    #[serde(rename = "ProductID")]
    pub product_id: ProductId,

    #[serde(rename = "BundleID")]
    pub bundle_id: BundleId,

    #[serde(rename = "Receipt")]
    pub receipt: String,
}

/// Subscription details
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NewSubscription {
    pub cycle: PlanCycle,

    pub currency: Option<Currency>,
    #[serde(rename = "CurrencyID")]
    pub currency_id: Option<i32>,

    pub plans: Option<HashMap<String, i32>>,
    #[serde(rename = "PlanIDs")]
    pub plan_ids: Option<Vec<i32>>,

    pub codes: Option<Vec<String>>,
    pub coupon_code: Option<String>,
    pub gift_code: Option<String>,
}

/// New subscription values
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct NewSubscriptionValues {
    pub amount: Option<u64>,
    pub payments: Option<Vec<String>>,
    pub payment_token: Option<String>,
}
