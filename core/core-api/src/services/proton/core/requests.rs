//! Request structures for the Proton Core API.
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

use crate::MAX_PAGE_ELEMENT_COUNT;
use crate::services::proton::prelude::*;
use serde::Serialize;
use serde_with::{BoolFromInt, serde_as};
use smart_default::SmartDefault;

use super::{DeviceEnvironment, LabelType};

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
    pub email: Option<PrivateEmail>,

    /// Label ID to filter on.
    #[serde(rename = "LabelID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_id: Option<LabelId>,

    /// Page index, i.e. the page in the resultset.
    pub page: u64,

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
    pub label_id: Option<LabelId>,

    /// Page index, i.e. the page in the resultset.
    pub page: u64,

    /// Number of records per page.
    #[default(MAX_PAGE_ELEMENT_COUNT)]
    pub page_size: usize,
}

/// Parameters for getting an event.
#[serde_as]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetEventOptions {
    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub conversation_counts: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub message_counts: bool,
}

impl GetEventOptions {
    /// Return an instance of `GetEventOptions` with all counts set to `true`.
    #[must_use]
    pub fn all() -> Self {
        Self {
            conversation_counts: true,
            message_counts: true,
        }
    }

    /// Return an instance of `GetEventOptions` with all counts set to `false`.
    #[must_use]
    pub fn no_counts() -> Self {
        Self {
            conversation_counts: false,
            message_counts: false,
        }
    }
}

/// Parameters for getting all keys.
#[serde_as]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetKeysAllOptions {
    /// The email address to get keys for.
    pub email: PrivateEmail,

    /// Whether to only get internal keys.
    #[serde_as(as = "Option<BoolFromInt>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_only: Option<bool>,
}

/// Parameters for getting images logo.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct GetImagesLogoOptions {
    /// The percent encoded address. Either Domain or Address are required.
    /// Ex: `Address=noreply%40amazon.com`
    pub address: Option<PrivateEmail>,

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

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct PutDeleteContacts {
    #[serde(rename = "IDs")]
    /// The list of contact IDs to delete.
    pub ids: Vec<ContactId>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostLabelsRequest {
    /// TODO: Document this field.
    #[serde(rename = "ParentID")]
    pub parent_id: Option<LabelId>,

    /// TODO: Document this field.
    pub color: String,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub label_type: LabelType,

    /// TODO: Document this field.
    pub name: String,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutLabelRequest {
    /// TODO: Document this field.
    #[serde(rename = "ParentID")]
    pub parent_id: Option<LabelId>,

    /// TODO: Document this field.
    pub color: String,

    /// TODO: Document this field.
    pub name: String,
}

/// TODO: Document this struct
#[serde_as]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PatchLabelRequest {
    /// TODO: Document this field.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub expanded: Option<bool>,
    /// TODO: Document this field.
    #[serde_as(as = "Option<BoolFromInt>")]
    pub notify: Option<bool>,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetLabelsOptions {
    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub label_type: LabelType,
}

/// Represents `POST /labels/by-ids` request body.
///
/// Name refers to the fact it actually gets labels by their IDs.
/// But due to the fact GET requests are not supposed to have a body
/// The struct is used with the POST method instead.
///
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetLabelsByIdsOptions {
    /// Label IDs to get.
    #[serde(rename = "LabelIDs")]
    pub label_ids: Vec<LabelId>,
}

/// Represents `POST /devices` request body.
///
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct RegisterDeviceRequest {
    /// Device token
    pub device_token: String,
    /// Environment to which we register
    pub environment: DeviceEnvironment,
    /// PGP Public Key
    pub public_key: Option<String>,
    /// TODO: Document this field
    pub ping_notification_status: Option<i32>,
    /// TODO: Document this field
    pub push_notification_status: Option<i32>,
}

/// Represents `POST /report/bug` request body
///
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostReportBug {
    /// OS name
    #[serde(rename = "OS")]
    pub os: String,
    /// OS version
    #[serde(rename = "OSVersion")]
    pub os_version: String,
    /// Client application name
    pub client: String,
    /// Version of client application
    pub client_version: String,
    /// Client application type (1 = Email)
    pub client_type: u8,
    /// Generic title
    pub title: String,
    /// Description of the bug
    pub description: String,
    /// Username (empty for no username)
    pub username: String,
    /// Email, must be a valid email address
    pub email: String,
    /// Logs (filename, zipped bytes)
    pub logs: Option<(String, Vec<u8>)>,
}

/// Represents a key to update in the password change request.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateKeyRequest {
    /// The ID of the key to update.
    #[serde(rename = "ID")]
    pub id: String,
    /// The new private key data.
    pub private_key: String,
}

/// Represents authentication input for key setup (copied from account-api).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthInput {
    /// The version of the authentication.
    pub version: u8,

    /// The modulus ID for authentication.
    #[serde(rename = "ModulusID")]
    pub modulus_id: String,

    /// The salt used in authentication.
    pub salt: String,

    /// The verifier for authentication.
    pub verifier: String,
}

/// Represents `PUT /keys/private` request body for password changes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutKeysPrivateRequest {
    /// Base64-encoded salt for key derivation (required).
    pub key_salt: String,

    /// Array of legacy keys to update (optional, for non-migrated users).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<UpdateKeyRequest>>,

    /// Array of user keys to update (optional, for migrated users).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_keys: Option<Vec<UpdateKeyRequest>>,

    /// Authentication information object.
    pub auth: AuthInput,
}

/// Represents FIDO2 authentication data for password change requests.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Fido2AuthData {
    /// Authentication options.
    pub authentication_options: serde_json::Value,
    /// Client data.
    pub client_data: String,
    /// Authenticator data.
    pub authenticator_data: String,
    /// Signature.
    pub signature: String,
    /// Credential ID.
    #[serde(rename = "CredentialID")]
    pub credential_id: Vec<Option<serde_json::Value>>,
}

/// Represents `PUT /core/v4/users/password` request body for password change authentication.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutUsersPasswordRequest {
    /// Base64-encoded client ephemeral value.
    pub client_ephemeral: String,
    /// Base64-encoded client proof.
    pub client_proof: String,
    /// Hex-encoded SRP session ID.
    #[serde(rename = "SRPSession")]
    pub srp_session: String,
    /// Two-factor authentication code (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub two_factor_code: Option<String>,
    /// FIDO2 authentication data (optional).
    #[serde(rename = "FIDO2")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fido2: Option<Fido2AuthData>,
    /// SSO re-authentication token (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sso_reauth_token: Option<String>,
}
