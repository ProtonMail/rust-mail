//! Request structures for the Proton Payments API.
//!
//! This module provides structures that are used to make requests to the Proton
//! API. These structures are used to define the request bodies and URL
//! parameters that are sent to the API when making a request.
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
//! Any types that are children of the primary request structures should be
//! defined separately in the [`request_data`](crate::services::proton::request_data)
//! module, or in the [`common`](crate::services::proton::common) module if they
//! used by both requests and responses.
//!

use crate::services::proton::prelude::*;
use serde::Serialize;
use smart_default::SmartDefault;

/// Parameters for getting payment plans.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetPaymentsPlansOptions {
    pub currency: Option<String>,
    pub vendor: Option<String>,
    pub state: Option<u8>,
    pub timestamp: Option<u64>,
    pub fallback: Option<bool>,
}

/// Parameters required to create a payment token.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PostPaymentsTokensRequest {
    pub amount: u64,
    pub currency: String,
    pub payment: PaymentReceipt,
}

/// Parameters required to create a payment subscription.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PostPaymentsSubscriptionRequest {
    #[serde(flatten)]
    pub subscription: NewSubscription,

    #[serde(flatten)]
    pub new_values: NewSubscriptionValues,
}
