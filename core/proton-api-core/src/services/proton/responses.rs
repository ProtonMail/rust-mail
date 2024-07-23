//! Response structures for the Proton API.
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

use crate::auth::SecretString;
use crate::services::proton::common::RemoteId;
use crate::services::proton::response_data::{
    Address, ContactBasic, ContactEmail, ContactFull, PasswordMode, Salt, TfaInfo, User,
    UserSettings,
};
use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup as PublicAddressKeyGroup,
    APIUnverifiedPublicAddressKeyGroup as UnverifiedPublicAddressKeyGroup,
};
use serde::Deserialize;
#[cfg(any(test, feature = "integration_tests"))]
use serde::Serialize;
use serde_with::serde_as;

/// The response containing addresses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetAddressesResponse {
    /// The list of addresses.
    pub addresses: Vec<Address>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetContactResponse {
    /// TODO: Document this field.
    pub contact: ContactFull,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetContactsEmailsResponse {
    /// TODO: Document this field.
    pub contact_emails: Vec<ContactEmail>,

    /// TODO: Document this field.
    pub total: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetContactsResponse {
    /// TODO: Document this field.
    pub contacts: Vec<ContactBasic>,

    /// TODO: Document this field.
    pub total: u64,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetEventsLatestResponse {
    /// TODO: Document this field.
    #[serde(rename = "EventID")]
    pub event_id: RemoteId,
}

/// Available public keys.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
#[serde_as]
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
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetKeysSaltsResponse {
    /// TODO: Document this field.
    pub key_salts: Vec<Salt>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetSettingsResponse {
    /// TODO: Document this field.
    #[serde(rename = "UserSettings")]
    pub user_settings: UserSettings,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct GetUsersResponse {
    /// TODO: Document this field.
    pub user: User,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthResponse {
    /// TODO: Document this field.
    pub access_token: SecretString,

    /// TODO: Document this field.
    pub password_mode: PasswordMode,

    /// TODO: Document this field.
    pub refresh_token: SecretString,

    /// TODO: Document this field.
    pub scope: String,

    /// TODO: Document this field.
    pub server_proof: String,

    /// TODO: Document this field.
    #[serde(rename = "2FA")]
    pub tfa: TfaInfo,

    /// TODO: Document this field.
    pub token_type: Option<String>,

    /// TODO: Document this field.
    #[serde(rename = "UID")]
    pub uid: RemoteId,

    /// TODO: Document this field.
    #[serde(rename = "UserID")]
    pub user_id: RemoteId,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthRefreshResponse {
    /// TODO: Document this field.
    pub access_token: SecretString,

    /// TODO: Document this field.
    pub refresh_token: SecretString,

    /// TODO: Document this field.
    pub scope: String,

    /// TODO: Document this field.
    pub token_type: Option<String>,

    /// TODO: Document this field.
    #[serde(rename = "UID")]
    pub uid: RemoteId,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthInfoResponse {
    /// TODO: Document this field.
    pub modulus: String,

    /// TODO: Document this field.
    pub salt: String,

    /// TODO: Document this field.
    pub server_ephemeral: String,

    /// TODO: Document this field.
    #[serde(rename = "SRPSession")]
    pub srp_session: String,

    /// TODO: Document this field.
    pub version: u8,
}

/// Fork session response.
///
/// This is the "selector" that is returned when a session is forked.
///
/// The relevant API documentation is here:
///
///   - [API docs](https://protonmail.gitlab-pages.protontech.ch/Slim-API/auth/#tag/Authentication-Sessions/operation/post_auth-%7B_version%7D-sessions-forks)
///
/// The fields in the JSON response are not currently documented.
///
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "integration_tests"), derive(Serialize))]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthSessionsForksResponse {
    /// The selector that is returned when a session is forked. It's not clear
    /// exactly what this is at present.
    pub selector: String,
}

//  TRAITS
//==============================================================================

/// Marker trait for individual event responses.
pub trait GetEventResponse: Send + Sync {}
