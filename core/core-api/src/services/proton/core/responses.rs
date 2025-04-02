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
    APIUnverifiedPublicAddressKeyGroup as UnverifiedPublicAddressKeyGroup, ArmoredPrivateKey, KeyId,
};
use serde::Deserialize;
use serde_repr::Deserialize_repr;
use serde_with::{serde_as, BoolFromInt};

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

/// Represents the response for getting available domains.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct GetAvailableDomainsResponse {
    /// The response code indicating the status of the request.
    /// A value of 1000 typically indicates success.
    pub code: ResponseCode,

    /// A list of available domain names.
    pub domains: Vec<String>,
}

/// Represents the response for setting up keys for a new private user account.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SetupKeysResponse {
    pub code: ResponseCode,
    pub user: User,
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

/// Represents the response to a request for setting up a new non-subscriber user address.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PostSetupNewNonSubuserAddressResponse {
    /// The response code indicating the success or failure of the request (e.g., 1000 for success).
    pub code: ResponseCode,

    /// The details of the newly created address.
    pub address: SetupNewNonSubuserAddressResponseAddress,
}

/// Represents the details of a newly created address for a non-subscriber user.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SetupNewNonSubuserAddressResponseAddress {
    /// The unique identifier of the address.
    #[serde(rename = "ID")]
    pub id: String,

    /// The unique identifier of the domain associated with the address.
    #[serde(rename = "DomainID")]
    pub domain_id: String,

    /// The full email address created (e.g., "user@domain.com").
    pub email: String,

    /// Indicates the sending capability level of the address (higher values may indicate priority).
    pub send: i32,

    /// The enabled status of the address (e.g., enabled or disabled).
    pub status: EnabledStatus,

    /// The type of address (e.g., original, alias, etc.).
    #[serde(rename = "Type")]
    pub addr_type: AddressType,

    /// Indicates whether the address can receive emails (active or inactive).
    pub receive: ReceiveStatus,

    /// The order or priority of the address among multiple addresses.
    pub order: i32,

    /// The display name associated with the address.
    pub display_name: String,

    /// The signature associated with the address for outgoing emails.
    pub signature: String,

    /// Indicates whether the address has associated encryption keys.
    pub has_keys: HasKeysStatus,

    /// A list of encryption key identifiers associated with the address.
    pub keys: Vec<String>,
}

/// Represents the enabled status of an address.
///
/// This enum indicates whether an address is currently enabled or disabled.
#[derive(Clone, Debug, PartialEq, Deserialize_repr)]
#[repr(u8)]
pub enum EnabledStatus {
    /// The address is disabled.
    Disabled = 0,
    /// The address is enabled.
    Enabled = 1,
}

/// Represents the receiving status of an address.
#[derive(Clone, Debug, PartialEq, Deserialize_repr)]
#[repr(u8)]
pub enum ReceiveStatus {
    /// The address is inactive.
    Inactive = 0,
    /// The address is active.
    Active = 1,
}

/// Represents whether an address has associated encryption keys.
#[derive(Clone, Debug, PartialEq, Deserialize_repr)]
#[repr(u8)]
pub enum HasKeysStatus {
    /// The address has no associated encryption keys.
    NoKeys = 0,
    /// The address has one or more associated encryption keys.
    HasKeys = 1,
}

//  TRAITS
//==============================================================================

/// Marker trait for individual event responses.
pub trait GetEventResponse: Send + Sync {}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_crypto_account::keys::{EncryptedKeyToken, KeyTokenSignature, LockedKey, UserKeys};
    use serde_json;

    #[test]
    fn test_setup_new_nonsubuser_address_deserialization() {
        let json = r#"
        {
            "Code": 1000,
            "Address": {
                "ID": "vuGSa1zsx0kV0jsfhX_xKSDQ0dvcLdMduA_c2c9fhaC1ZYCZKe8gony7nIWbnqaj8gebXLCQre1H1ZTKkhhFxA==",
                "DomainID": "X_bSECsnvCSHHR44lXWMDOYDiZpbTUzqnQFyf_pqDq-JjXxXJCv_jQmSOLhD3e3A==",
                "Email": "me@protonmail.com",
                "Send": 3,
                "Status": 1,
                "Type": 1,
                "Receive": 0,
                "Order": 1,
                "DisplayName": "hi",
                "Signature": "signature",
                "HasKeys": 0,
                "Keys": ["key1", "key2"]
            }
        }
        "#;

        let response: PostSetupNewNonSubuserAddressResponse = serde_json::from_str(json).expect("Failed to deserialize JSON");

        let expected = PostSetupNewNonSubuserAddressResponse {
            code: ResponseCode(1000),
            address: SetupNewNonSubuserAddressResponseAddress {
                id: "vuGSa1zsx0kV0jsfhX_xKSDQ0dvcLdMduA_c2c9fhaC1ZYCZKe8gony7nIWbnqaj8gebXLCQre1H1ZTKkhhFxA==".to_string(),
                domain_id: "X_bSECsnvCSHHR44lXWMDOYDiZpbTUzqnQFyf_pqDq-JjXxXJCv_jQmSOLhD3e3A==".to_string(),
                email: "me@protonmail.com".to_string(),
                send: 3,
                status: EnabledStatus::Enabled,
                addr_type: AddressType::Original,
                receive: ReceiveStatus::Inactive,
                order: 1,
                display_name: "hi".to_string(),
                signature: "signature".to_string(),
                has_keys: HasKeysStatus::NoKeys,
                keys: vec![String::from("key1"), String::from("key2")],
            },
        };

        assert_eq!(response, expected);
    }


    #[test]
    fn test_get_available_domains_deserialization() {
        let json = r#"
        {
            "Code": 1000,
            "Domains": ["proton.me", "protonmail.com", "example.com"]
        }
        "#;

        let response: GetAvailableDomainsResponse =
            serde_json::from_str(json).expect("Failed to deserialize JSON");

        let expected = GetAvailableDomainsResponse {
            code: ResponseCode(1000),
            domains: vec![
                String::from("proton.me"),
                String::from("protonmail.com"),
                String::from("example.com"),
            ],
        };

        assert_eq!(response, expected);
    }

    #[test]
    fn test_setup_keys_request_deserialize_with_all_fields() {
        let json = r#"{
            "PrimaryKey": "primary_key_example",
            "KeySalt": "random_salt",
            "OrgPrimaryUserKey": "encrypted_key",
            "OrgActivationToken": "32bytehextoken",
            "AddressKeys": [{"AddressID": "addr_id_1", "PrivateKey": "addr_private_key", "Token": "addr_token", "Signature": "addr_signature","SignedKeyList":{"Data":"data","Signature":"signature"}}],
            "Auth": {"Version": 2, "ModulusID": "modulus_id", "Salt": "auth_salt", "Verifier": "auth_verifier"},
            "AddressList": {"Revision": 1, "Data": "{\"key\":\"value\"}", "Signature": "signed_list_signature"},
            "EncryptedSecret": "base64_encrypted_secret"
        }"#;
        let expected = SetupKeysRequest {
            primary_key: "primary_key_example".to_string(),
            key_salt: "random_salt".to_string(),
            org_primary_user_key: Some("encrypted_key".to_string()),
            org_activation_token: Some("32bytehextoken".to_string()),
            address_keys: vec![AddressKeyInput {
                address_id: "addr_id_1".to_string(),
                private_key: "addr_private_key".to_string(),
                token: Some("addr_token".to_string()),
                signature: Some("addr_signature".to_string()),
                signed_key_list: SignedKeyList { data: "data".to_string(), signature: "signature".to_string() },
                revision: 0,
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

        let deserialized: SetupKeysRequest = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(deserialized, expected);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_setup_keys_response_deserialize_with_full_user() {
        let json = r#"{
            "Code": 1000,
            "User": {
                "ID": "user_id",
                "Name": "username",
                "DisplayName": "User Name",
                "Email": "user@example.com",
                "Currency": "USD",
                "Credit": 100,
                "Type": 1,
                "CreateTime": 1234567890,
                "MaxSpace": 1073741824,
                "MaxUpload": 10485760,
                "UsedSpace": 5242880,
                "ProductUsedSpace": {
                    "Calendar": 1024,
                    "Contact": 2048,
                    "Drive": 3072,
                    "Mail": 4096,
                    "Pass": 512
                },
                "NumAI": 1,
                "NumLumo": 0,
                "Role": 1,
                "Private": 1,
                "ToMigrate": 0,
                "MnemonicStatus": 3,
                "Subscribed": 1,
                "Services": 1,
                "Delinquent": 0,
                "Keys": [
                    {
                        "ID": "key_id",
                        "Version": 1,
                        "PrivateKey": "private_key",
                        "Primary": 1,
                        "Active": 1,
                        "RecoverySecret": "recovery_secret",
                        "RecoverySecretSignature": "recovery_secret_signature",
                        "Token": "token",
                        "Signature": "signature",
                        "Activation": "activation",
                        "AddressForwardingID": "address_forwarding_id"
                    }
                ],
                "Flags": {
                    "protected": true,
                    "onboard-checklist-storage-granted": false,
                    "has-temporary-password": false,
                    "test-account": false,
                    "no-login": false,
                    "recovery-attempt": false,
                    "sso": false,
                    "no-proton-address": false
                }
            }
        }"#;
        let expected = SetupKeysResponse {
            code: ResponseCode(1000),
            user: User {
                id: UserId::from("user_id"),
                role: 1,
                name: Some("username".to_string()),
                display_name: Some("User Name".to_string()),
                email: "user@example.com".to_string(),
                currency: "USD".to_string(),
                credit: 100,
                user_type: UserType::Proton,
                create_time: 1_234_567_890,
                max_space: 1_073_741_824,
                max_upload: 10_485_760,
                used_space: 5_242_880,
                product_used_space: ProductUsedSpace {
                    calendar: 1024,
                    contact: 2048,
                    drive: 3072,
                    mail: 4096,
                    pass: 512,
                },
                private: 1,
                to_migrate: false,
                mnemonic_status: UserMnemonicStatus::EnabledAndSet,
                subscribed: 1,
                services: 1,
                delinquent: DelinquentState::Paid,
                keys: UserKeys::new(vec![LockedKey {
                    id: KeyId::from("key_id"),
                    version: 1,
                    private_key: ArmoredPrivateKey::from("private_key"),
                    primary: true,
                    active: true,
                    recovery_secret: Some("recovery_secret".to_string()),
                    recovery_secret_signature: Some("recovery_secret_signature".to_string()),
                    flags: None,
                    token: Some(EncryptedKeyToken::from("token")),
                    signature: Some(KeyTokenSignature::from("signature")),
                    activation: Some(String::from("activation")),
                    address_forwarding_id: Some(String::from("address_forwarding_id")),
                    
                }]),
                flags: Flags {
                    protected: true,
                    onboard_checklist_storage_granted: false,
                    has_temporary_password: false,
                    test_account: false,
                    no_login: false,
                    recovery_attempt: false,
                    sso: false,
                    no_proton_address: false,
                },
            },
        };

        let deserialized: SetupKeysResponse = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(deserialized, expected);
    }
}
