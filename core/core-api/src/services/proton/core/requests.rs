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
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
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
    pub email: Option<String>,

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

impl GetEventOptions {
    /// Return an instance of `GetEventOptions` with all counts set to `true`.
    #[must_use]
    pub fn all() -> Self {
        Self {
            conversation_counts: true,
            message_counts: true,
        }
    }
}

/// Parameters for getting all keys.
#[serde_as]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetKeysAllOptions {
    /// The email address to get keys for.
    pub email: String,

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

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, SmartDefault)]
#[serde(rename_all = "PascalCase")]
pub struct PutDeleteContacts {
    #[serde(rename = "IDs")]
    /// The list of contact IDs to delete.
    pub ids: Vec<ContactId>,
}

/// Represents a request to set up a new address for a non-subscriber user.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostSetupNewNonSubuserAddressRequest {
    /// The domain part of the email address, either a custom domain or a `ProtonMail` domain.
    pub domain: String,

    /// The display name associated with the new address.
    pub display_name: String,

    /// The signature to be associated with the new address.
    pub signature: String,

    /// The unique identifier of the member for whom the address is being created.
    #[serde(rename = "MemberID")]
    pub member_id: String,

    /// The unique identifier of the member requesting the address creation, if applicable.
    pub requester_member_id: Option<String>,

    /// A list of additional addresses or aliases related to this setup.
    pub address_list: Vec<String>,
}

/// Represents a signed key with its data and signature.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SignedKeyList {
    /// JSON-encoded content of the SAL
    pub data: String,

    /// The armored signature over the JSON-serialized data with the primary user key
    pub signature: String,
}


/// Represents the query parameters for the "Get available domains" request.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct GetAvailableDomainsRequest {
    /// The type filter for domains. If None, no specific type is requested.
    /// Can be a string to filter domains by type, or null to include all types.
    #[serde(rename = "Type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_type: Option<String>,
}


/// Represents the query parameters for checking if a username is already taken.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CheckUsernameRequest {
    /// The username to check for availability.
    /// Defaults to an empty string if not provided.
    #[serde(default)]
    pub name: String,

    /// Indicates whether the username should be parsed as a full email address.
    /// Defaults to `NoEmail` (0) if not provided.
    #[serde(default)]
    pub parse_domain: ParseDomain,
}

/// Represents the query parameters for checking if an external username is already taken.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CheckExternalUsernameRequest {
    /// The username to check for availability.
    /// Defaults to an empty string if not provided.
    #[serde(default)]
    pub name: String,
}

/// Indicates whether the username should be parsed as a full email address.
#[derive(Clone, Debug, PartialEq, Deserialize_repr, Serialize_repr, Default)]
#[repr(u8)]
pub enum ParseDomain {
    /// The username is not a full email address (default).
    #[default]
    NoEmail = 0,
    /// The username is a full email address.
    FullEmail = 1,
}


/// Represents the type of verification code delivery method.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationType {
    /// Verification code sent via email.
    Email,
    /// Verification code sent via SMS.
    Sms,
}

/// Represents the destination details for sending a verification code.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Destination {
    /// The email address to send the verification code to.
    /// Required if the type is "email".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,

    /// The phone number to send the verification code to.
    /// Required if the type is "sms".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

/// Represents the query parameters for sending a verification code.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SendVerificationCodeRequest {
    pub username: String,
    /// The type of verification method (email or sms).
    #[serde(rename = "Type")]
    pub verification_type: VerificationType,

    /// The platform for the verification link, optional.
    /// Can be "android" or other supported platforms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    /// The destination details for the verification code.
    pub destination: Destination,
}

/// Represents an address key input for key setup.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddressKeyInput {
    /// The address ID.
    #[serde(rename = "AddressID")]
    pub address_id: String,

    /// The private key for the address.
    pub private_key: String,

    /// The token associated with the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// The signature of the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,

    /// Signed key list
    pub signed_key_list: SignedKeyList,

    #[serde(default)]
    pub revision: i32,
}

/// Represents a signed key list input for address setup.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SignedAddressList {
    /// JSON-encoded content of the SAL (Signed Address List).
    pub data: String,
    /// The armored signature over the JSON-serialized data with the primary user key.
    pub signature: String,
}

/// Represents authentication input for key setup.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthInput {
    /// The version of the authentication.
    pub version: i32,

    /// The modulus ID for authentication.
    #[serde(rename = "ModulusID")]
    pub modulus_id: String,

    /// The salt used in authentication.
    pub salt: String,

    /// The verifier for authentication.
    pub verifier: String,
}

pub enum AsyncUserInitialization {
    CalledByClient,
    Other,
}

impl From<AsyncUserInitialization> for i32 {
    fn from(value: AsyncUserInitialization) -> Self {
        match value {
            AsyncUserInitialization::CalledByClient => 1,
            AsyncUserInitialization::Other => 0,
        }
    }
}
/// Represents the query parameters for setting up keys for a new private user account.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SetupKeysRequest {
    /// The primary key for the user.
    pub primary_key: String,
    /// A randomly generated client-side key salt.
    pub key_salt: String,
    /// The primary key encrypted to the token in `OrgActivationToken` (for magic link setup).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_primary_user_key: Option<String>,
    /// A 32-byte random token encoded as hex, encrypted to the organization key and signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_activation_token: Option<String>,
    /// List of address keys for the account.
    pub address_keys: Vec<AddressKeyInput>,
    /// Authentication details for the setup.
    pub auth: AuthInput,
    /// Signed list of all addresses.
    pub address_list: Option<SignedAddressList>,
    /// Base64-encoded AES-GCM encrypted secret using the `DeviceSecret` as key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_secret: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_get_available_domains_request_serialization() {
        let request_with_type = GetAvailableDomainsRequest {
            domain_type: Some("custom".to_string()),
        };
        let serialized = serde_json::to_string(&request_with_type).expect("Failed to serialize");
        assert_eq!(serialized, r#"{"Type":"custom"}"#);

        let request_no_type = GetAvailableDomainsRequest { domain_type: None };
        let serialized_no_type = serde_json::to_string(&request_no_type).expect("Failed to serialize");
        assert_eq!(serialized_no_type, "{}");
    }

    #[test]
    fn test_check_username_request_serialize_with_name_and_full_email() {
        let request = CheckUsernameRequest {
            name: "bart".to_string(),
            parse_domain: ParseDomain::FullEmail,
        };
        let expected_json = r#"{"Name":"bart","ParseDomain":1}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }


    #[test]
    fn test_check_username_request_serialize_with_name_only() {
        let request = CheckUsernameRequest {
            name: "bart".to_string(),
            parse_domain: ParseDomain::NoEmail,
        };
        let expected_json = r#"{"Name":"bart","ParseDomain":0}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_check_username_request_serialize_with_default_values() {
        let request = CheckUsernameRequest {
            name: String::new(),
            parse_domain: ParseDomain::NoEmail,
        };
        let expected_json = r#"{"Name":"","ParseDomain":0}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_send_verification_code_request_serialize_with_email_and_platform() {
        let request = SendVerificationCodeRequest {
            verification_type: VerificationType::Email,
            platform: Some("android".to_string()),
            destination: Destination {
                address: Some("user@example.com".to_string()),
                phone: None,
            },
            username: "name".to_owned(),
        };
        let expected_json = r#"{"Username":"name","Type":"email","Platform":"android","Destination":{"Address":"user@example.com"}}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_send_verification_code_request_serialize_with_sms_only() {
        let request = SendVerificationCodeRequest {
            verification_type: VerificationType::Sms,
            platform: None,
            destination: Destination {
                address: None,
                phone: Some("+1234567890".to_string()),
            },
            username: "name".to_owned(),
        };
        let expected_json = r#"{"Username":"name","Type":"sms","Destination":{"Phone":"+1234567890"}}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_send_verification_code_request_serialize_with_email_only() {
        let request = SendVerificationCodeRequest {
            verification_type: VerificationType::Email,
            platform: None,
            destination: Destination {
                address: Some("user@example.com".to_string()),
                phone: None,
            },
            username: "name".to_owned(),
        };
        let expected_json = r#"{"Username":"name","Type":"email","Destination":{"Address":"user@example.com"}}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_setup_keys_request_serialize_with_all_fields() {
        let request = SetupKeysRequest {
            primary_key: "primary_key_example".to_string(),
            key_salt: "random_salt".to_string(),
            org_primary_user_key: Some("encrypted_key".to_string()),
            org_activation_token: Some("32bytehextoken".to_string()),
            address_keys: vec![AddressKeyInput {
                address_id: "addr_id_1".to_string(),
                private_key: "addr_private_key".to_string(),
                token: Some("addr_token".to_string()),
                signature: Some("addr_signature".to_string()),
                signed_key_list: SignedKeyList { data: String::from("data"), signature: String::from("signature") },
                revision: 3,
            }],
            auth: AuthInput {
                version: 2,
                modulus_id: "modulus_id".to_string(),
                salt: "auth_salt".to_string(),
                verifier: "auth_verifier".to_string(),
            },
            address_list: Some(SignedAddressList {
                data: r#"{"key":"value"}"#.to_string(),
                signature: "signed_list_signature".to_string(),
            }),
            encrypted_secret: Some("base64_encrypted_secret".to_string()),
        };
        let expected_json = r#"{
            "PrimaryKey": "primary_key_example",
            "KeySalt": "random_salt",
            "OrgPrimaryUserKey": "encrypted_key",
            "OrgActivationToken": "32bytehextoken",
            "AddressKeys": [{"AddressID": "addr_id_1", "PrivateKey": "addr_private_key","Token": "addr_token", "Signature": "addr_signature","SignedKeyList":{"Data":"data","Signature":"signature"},"Revision": 3}],
            "Auth": {"Version": 2, "ModulusID": "modulus_id", "Salt": "auth_salt", "Verifier": "auth_verifier"},
            "AddressList": {"Data": "{\"key\":\"value\"}", "Signature": "signed_list_signature"},
            "EncryptedSecret": "base64_encrypted_secret"
        }"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json.replace(['\n', ' '], ""));
    }
}
