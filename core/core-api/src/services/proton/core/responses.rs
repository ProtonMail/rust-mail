//! Response structures for the Proton Core API.
//!
//! This module provides structures that are used to receive responses from the
//! Proton API. These structures are used to define the response bodies that are
//! received from the API when making a request.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the structs in this module should mirror the API endpoint response
//! definitions, and NOT have any business logic or other functionality.
//!
//! To be clear, they should only contain data, and not methods; should not be
//! saved in the database; and should not be used for anything except providing
//! an interface for incoming data.
//!
//! Structs in this module should only implement [`Deserialize`], and should not
//! implement [`Serialize`](serde::Serialize). If anything in this module
//! implements [`Serialize`](serde::Serialize), it is a sign that a mistake has
//! been made. The exception here is for testing purposes, e.g. when mocking
//! response data — in which case implementing [`Serialize`](serde::Serialize)
//! conditionally, only in test mode, is advised.
//!
//! Any types that are children of the primary response structures should be
//! defined separately in the [`response_data`](crate::services::proton::response_data)
//! module, or in the [`common`](crate::services::proton::common) module if they
//! are used by both requests and responses.
//!

use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup as PublicAddressKeyGroup,
    APIUnverifiedPublicAddressKeyGroup as UnverifiedPublicAddressKeyGroup, ArmoredPrivateKey,
    KeyId,
};
use serde::Deserialize;
use serde_with::{BoolFromInt, serde_as};

#[cfg(any(test, debug_assertions))]
use serde::Serialize;

use crate::services::proton::common::ApiErrorInfo;
use crate::services::proton::prelude::*;

/// The response code indicating the status of the request.
/// A value of 1000 typically indicates success.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ResponseCode(i32);

/// The response containing addresses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetAddressesResponse {
    /// The list of addresses.
    pub addresses: Vec<Address>,
}

/// The response containing an address.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetAddressResponse {
    /// The list of addresses.
    pub address: Address,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetContactResponse {
    /// TODO: Document this field.
    pub contact: ContactFull,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetContactsEmailsResponse {
    /// TODO: Document this field.
    pub contact_emails: Vec<ContactEmail>,

    /// TODO: Document this field.
    pub total: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetContactsResponse {
    /// TODO: Document this field.
    pub contacts: Vec<ContactBasic>,

    /// TODO: Document this field.
    pub total: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetEventsLatestResponse {
    /// TODO: Document this field.
    #[serde(rename = "EventID")]
    pub event_id: EventId,
}

/// Available public keys.
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetKeysAllResponse {
    /// Information about the internal address itself, if it exists. Since the
    /// SKL is mandatory, this will never be nullable.
    #[serde(rename = "Address")]
    pub address_keys: PublicAddressKeyGroup,

    /// Information about the catch-all address itself, if it exists. This can
    /// be null if the address keys are valid
    #[serde(rename = "CatchAll")]
    pub catch_all_keys: Option<PublicAddressKeyGroup>,

    /// Tells whether this is an official Proton address.
    #[serde_as(as = "BoolFromInt")]
    pub is_proton: bool,

    /// True when domain has valid proton MX.
    #[serde(rename = "ProtonMX")]
    pub proton_mx: bool,

    /// Any other key that cannot be verified, such as Proton legacy keys or
    /// WKD.
    #[serde(rename = "Unverified")]
    pub unverified_keys: Option<UnverifiedPublicAddressKeyGroup>,

    /// List of warnings to show to the user related to phishing and message
    /// routing.
    pub warnings: Vec<String>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetKeysSaltsResponse {
    /// TODO: Document this field.
    pub key_salts: Vec<Salt>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetSettingsResponse {
    /// TODO: Document this field.
    pub user_settings: UserSettings,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetUsersResponse {
    /// TODO: Document this field.
    pub user: User,
}

/// The response containing information about deletion of the contacts
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutDeleteContactsResponse {
    /// List of responses.
    pub responses: Vec<PutDeleteContactResponse>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutDeleteContactResponse {
    /// Remote ID of the contact.
    #[serde(rename = "ID")]
    pub id: ContactId,
    /// Response data.
    pub response: ApiErrorInfo,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetLabelsResponse {
    /// TODO: Document this field.
    pub labels: Vec<Label>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostLabelsResponse {
    /// TODO: Document this field.
    pub label: Label,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PutLabelResponse {
    /// TODO: Document this field.
    pub label: Label,
}
/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PatchLabelResponse {
    /// TODO: Document this struct.
    pub label: Label,
}

/// Represents a user key in the response.
#[serde_as]
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserKey {
    /// Proton ID of the key.
    #[serde(rename = "ID")]
    pub id: KeyId,

    /// Proton version of the key.
    pub version: u32,

    /// `OpenPGP` private key armored.
    pub private_key: ArmoredPrivateKey,
    pub fingerprint: String,

    /// Is the key the primary key to use.
    #[serde_as(as = "BoolFromInt")]
    pub primary: bool,

    /// The key is active and should be decryptable.
    #[serde_as(as = "BoolFromInt")]
    pub active: bool,

    /// Secret for key recovery of a local file.
    pub recovery_secret: String,

    /// Signature for the recovery secret.
    pub recovery_secret_signature: String,

    /// Signature for the recovery secret.
    #[serde(default)]
    pub flags: u32,
}

//  TRAITS
//==============================================================================

/// Marker trait for individual event responses.
pub trait GetEventResponse: Send + Sync {}
