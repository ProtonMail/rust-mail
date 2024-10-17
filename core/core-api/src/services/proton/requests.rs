//! Request structures for the Proton API.
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

use crate::services::proton::common::{Fido2Auth, LightOrDarkMode, RemoteId};
use crate::MAX_PAGE_ELEMENT_COUNT;
use serde::Serialize;
use serde_with::{serde_as, BoolFromInt};
use smart_default::SmartDefault;

/// Parameters for getting Captcha details.
#[serde_as]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetCaptchaOptions {
    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub force_web_messaging: bool,

    /// The Captcha token to use.
    pub token: String,
}

/// Parameters for getting emails for contacts.
#[derive(Clone, Debug, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetContactsEmailsOptions {
    /// Email address to filter on
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Label ID to filter on.
    #[serde(rename = "LabelID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_id: Option<RemoteId>,

    /// Page index, i.e. the page in the resultset.
    pub page: usize,

    /// Number of records per page.
    #[default(MAX_PAGE_ELEMENT_COUNT)]
    pub page_size: usize,
}

/// Parameters for getting contacts.
#[derive(Clone, Debug, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetContactsOptions {
    /// Label ID to filter on.
    #[serde(rename = "LabelID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_id: Option<RemoteId>,

    /// Page index, i.e. the page in the resultset.
    pub page: usize,

    /// Number of records per page.
    #[default(MAX_PAGE_ELEMENT_COUNT)]
    pub page_size: usize,
}

/// Parameters for getting an event.
#[serde_as]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetEventOptions {
    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub conversation_counts: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub message_counts: bool,
}

/// Parameters for getting all keys.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetKeysAllOptions {
    /// The email address to get keys for.
    pub email: String,

    /// Whether to only get internal keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_only: Option<bool>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthRequest {
    /// TODO: Document this field.
    pub client_ephemeral: String,

    /// TODO: Document this field.
    pub client_proof: String,

    /// TODO: Document this field.
    #[serde(rename = "SRPSession")]
    pub srp_session: String,

    /// TODO: Document this field.
    pub username: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthInfoRequest {
    /// TODO: Document this field.
    pub username: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthRefreshRequest {
    /// TODO: Document this field.
    pub grant_type: String,

    /// TODO: Document this field.
    #[serde(rename = "RedirectURI")]
    pub redirect_uri: String,

    /// TODO: Document this field.
    pub refresh_token: String,

    /// TODO: Document this field.
    pub response_type: String,

    /// TODO: Document this field.
    #[serde(rename = "UID")]
    pub uid: RemoteId,
}

/// Fork session request.
///
/// This request is used to fork a user's session, providing a new session for
/// the same user.
///
/// The general documentation for this can currently be found here:
///
///   - [Feature documentation](https://confluence.protontech.ch/display/CP/How+to+generate+a+session+fork+selector+for+testing+the+lite+account+application)
///
/// The required POST request is described as being:
///
///   - `POST /api/auth/sessions/forks`
///   - `{ ChildClientID: "web-account-lite", Independent: 0 }`
///
/// The relevant API documentation is here:
///
///   - [API docs](https://protonmail.gitlab-pages.protontech.ch/Slim-API/auth/#tag/Authentication-Sessions/operation/post_auth-%7B_version%7D-sessions-forks)
///
/// The fields in the JSON body are not currently documented.
///
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthSessionsForksRequest {
    /// The child client ID, which is always `"web-account-lite"` at present. It
    /// seems like this is an identifier for the caller, but this is not clear.
    #[serde(rename = "ChildClientID")]
    pub child_client_id: String,

    /// It's not currently known what this does, and it's always set to `0`.
    pub independent: u8,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthTfaRequest {
    /// TODO: Document this field.
    pub two_factor_code: String,

    /// TODO: Document this field.
    pub fido2: Fido2Auth,
}

/// Parameters for getting images logo.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetImagesLogoOptions {
    /// The percent encoded address. Either Domain or Address are required.
    /// Ex: `Address=noreply%40amazon.com`
    pub address: Option<String>,

    /// The bimi-selector of the message
    pub bimi_selector: Option<String>,

    /// Domain to get the logo for. Either Domain or Address are required.
    /// Ex: `Domain=amazon.com`
    pub domain: Option<String>,

    /// Expected format for the image
    /// Ex: `Format=png`
    pub format: Option<String>,

    /// The maximum factor an image can be scaled up.
    /// Enum: 1, 2, 3 or 4
    /// Ex: `MaxScaleUpFactor=2`
    pub max_scale_up_factor: Option<u8>,

    /// The theme being used.
    /// Enum: `light` or `dark`
    pub mode: Option<LightOrDarkMode>,

    /// The size of the logo to be returned.
    pub size: Option<u32>,
}
