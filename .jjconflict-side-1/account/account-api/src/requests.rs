use crate::shared::challenge::ChallengePayload;
use muon::rest::auth::v4::fido2;
use proton_crypto_account::keys::{LocalAddressKey, LocalSignedKeyList};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// The type of account to create.
#[derive(Clone, Copy, Debug, Serialize_repr, Deserialize_repr, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum CreateUserType {
    Normal = 1,

    #[deprecated]
    Username = 2,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateUserRequest {
    /// The type of user being created (e.g., internal or external).
    #[serde(rename = "Type")]
    pub user_type: CreateUserType,

    /// The username to be created.
    pub username: String,

    /// The domain for the user, if applicable.
    pub domain: Option<String>,

    /// The auth input for user creation.
    pub auth: AuthInput,

    /// The email address associated with the user.
    pub email: Option<String>,

    /// The phone number associated with the user.
    pub phone: Option<String>,

    /// The referrer for the user, if any.
    pub referrer: Option<String>,

    /// The challenge payload, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<ChallengePayload>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateExternalUserRequest {
    /// The type of user being created (e.g., internal or external).
    pub user_type: CreateUserType,

    /// The email address associated with the external user.
    pub email: String,

    /// The auth input for user creation.
    pub auth: AuthInput,

    /// The referrer for the user, if any.
    pub referrer: Option<String>,
}

/// Represents a request to set up a new address for a non-subscriber user.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAuthRequest {
    pub username: Option<String>,
    pub client_ephemeral: Option<String>,
    pub client_proof: Option<String>,
    pub session: Option<String>,
    pub fingerprint: Option<String>,
}

/// Represents a request to set up a new address for a non-subscriber user.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PostAddressesSetupRequest {
    /// The domain part of the email address, either a custom domain or a `ProtonMail` domain.
    pub domain: String,

    /// The display name associated with the new address.
    pub display_name: Option<String>,

    /// The signature to be associated with the new address.
    pub signature: Option<String>,

    /// The unique identifier of the member for whom the address is being created.
    #[serde(rename = "MemberID")]
    pub member_id: Option<String>,

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
    pub name: String,

    /// Indicates whether the username should be parsed as a full email address.
    pub parse_domain: ParseDomain,
}

/// Represents the query parameters for checking if an external username is already taken.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CheckExternalUsernameRequest {
    /// The username to check for availability.
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

    pub primary: u8,

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

impl AddressKeyInput {
    #[must_use]
    pub fn new(addr_id: &str, addr_key: &LocalAddressKey, addr_skl: &LocalSignedKeyList) -> Self {
        let signed_key_list = SignedKeyList {
            data: addr_skl.data.to_string(),
            signature: addr_skl.signature.to_string(),
        };

        Self {
            address_id: addr_id.to_owned(),
            private_key: addr_key.private_key.to_string(),
            token: addr_key.token.clone().map(|t| t.to_string()),
            signature: addr_key.signature.clone().map(|t| t.to_string()),
            signed_key_list,
            revision: 0,
            primary: 1,
        }
    }
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
    pub version: u8,

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
    /// Authentication details for the setup.
    pub auth: AuthInput,
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
    /// Base64-encoded AES-GCM encrypted secret using the `DeviceSecret` as key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_secret: Option<String>,
}

/// Represents a request to validate an email address
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ValidateEmailRequest {
    pub email: String,
}

/// Represents a request to validate a phone number
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ValidatePhoneRequest {
    pub phone: String,
}

/// Represents a request to create a user key.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateUserKeyRequest {
    /// The private key for the user.
    pub private_key: String,

    /// Indicates if this is the primary key (1 for primary, 0 for non-primary).
    pub primary: u8,
}

/// Represents a request to create an address key.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateAddressKeyRequest {
    /// The address ID.
    #[serde(rename = "AddressID")]
    pub address_id: String,

    /// The private key for the address.
    pub private_key: String,

    /// The address forwarding ID.
    #[serde(rename = "AddressForwardingID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_forwarding_id: Option<String>,

    /// Indicates if this is the primary key (1 for primary, 0 for non-primary).
    pub primary: u8,

    /// The token associated with the key.
    pub token: String,

    /// The signature of the key.
    pub signature: String,

    /// Signed key list
    pub signed_key_list: SignedKeyList,
}

/// Represents `PUT /settings/password` request body for password changes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PutSettingsPasswordRequest {
    /// Authentication information object.
    pub auth: AuthInput,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthInput>,
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
    pub fido2: Option<fido2::Request>,
}

#[cfg(test)]
mod tests {
    use crate::shared::challenge::{Behavior, ChallengeInfo};

    use super::*;
    use proton_core_common::device::DeviceInfo;
    use serde_json;

    #[test]
    fn test_get_available_domains_request_serialization() {
        let request_with_type = GetAvailableDomainsRequest {
            domain_type: Some("custom".to_string()),
        };
        let serialized = serde_json::to_string(&request_with_type).expect("Failed to serialize");
        assert_eq!(serialized, r#"{"Type":"custom"}"#);

        let request_no_type = GetAvailableDomainsRequest { domain_type: None };
        let serialized_no_type =
            serde_json::to_string(&request_no_type).expect("Failed to serialize");
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
        let expected_json =
            r#"{"Username":"name","Type":"sms","Destination":{"Phone":"+1234567890"}}"#;

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
        let expected_json =
            r#"{"Username":"name","Type":"email","Destination":{"Address":"user@example.com"}}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_setup_keys_request_serialize_with_all_fields() {
        let request = SetupKeysRequest {
            auth: AuthInput {
                version: 2,
                modulus_id: "modulus_id".to_string(),
                salt: "auth_salt".to_string(),
                verifier: "auth_verifier".to_string(),
            },
            primary_key: "primary_key_example".to_string(),
            key_salt: "random_salt".to_string(),
            org_primary_user_key: Some("encrypted_key".to_string()),
            org_activation_token: Some("32bytehextoken".to_string()),
            address_keys: vec![AddressKeyInput {
                address_id: "addr_id_1".to_string(),
                private_key: "addr_private_key".to_string(),
                primary: 1,
                token: Some("addr_token".to_string()),
                signature: Some("addr_signature".to_string()),
                signed_key_list: SignedKeyList {
                    data: String::from("data"),
                    signature: String::from("signature"),
                },
                revision: 3,
            }],
            encrypted_secret: Some("base64_encrypted_secret".to_string()),
        };
        let expected_json = r#"{
            "Auth": { "Version": 2, "ModulusID": "modulus_id", "Salt": "auth_salt", "Verifier": "auth_verifier" },
            "PrimaryKey": "primary_key_example",
            "KeySalt": "random_salt",
            "OrgPrimaryUserKey": "encrypted_key",
            "OrgActivationToken": "32bytehextoken",
            "AddressKeys": [{ "AddressID": "addr_id_1", "PrivateKey": "addr_private_key", "Primary": 1, "Token": "addr_token", "Signature": "addr_signature", "SignedKeyList": { "Data": "data", "Signature": "signature" }, "Revision": 3 }],
            "EncryptedSecret": "base64_encrypted_secret"
        }"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json.replace(['\n', ' '], ""));
    }

    #[test]
    fn test_setup_keys_request_deserialize_with_all_fields() {
        let json = r#"{
            "PrimaryKey": "primary_key_example",
            "Primary": 1,
            "KeySalt": "random_salt",
            "OrgPrimaryUserKey": "encrypted_key",
            "OrgActivationToken": "32bytehextoken",
            "AddressKeys": [{"AddressID": "addr_id_1", "PrivateKey": "addr_private_key", "Primary": 1, "Token": "addr_token", "Signature": "addr_signature","SignedKeyList":{"Data":"data","Signature":"signature"}}],
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
                primary: 1,
                token: Some("addr_token".to_string()),
                signature: Some("addr_signature".to_string()),
                signed_key_list: SignedKeyList {
                    data: "data".to_string(),
                    signature: "signature".to_string(),
                },
                revision: 0,
            }],
            auth: AuthInput {
                version: 2,
                modulus_id: "modulus_id".to_string(),
                salt: "auth_salt".to_string(),
                verifier: "auth_verifier".to_string(),
            },
            encrypted_secret: Some("base64_encrypted_secret".to_string()),
        };

        let deserialized: SetupKeysRequest =
            serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(deserialized, expected);
    }

    #[test]
    fn test_validate_email_serialization() {
        let request_with_type = ValidateEmailRequest {
            email: "einstein@pm.me".to_string(),
        };
        let serialized = serde_json::to_string(&request_with_type).expect("Failed to serialize");
        assert_eq!(serialized, r#"{"Email":"einstein@pm.me"}"#);
    }

    #[test]
    fn test_validate_phone_serialization() {
        let request_with_type = ValidatePhoneRequest {
            phone: "+4915735774265".to_string(),
        };
        let serialized = serde_json::to_string(&request_with_type).expect("Failed to serialize");
        assert_eq!(serialized, r#"{"Phone":"+4915735774265"}"#);
    }

    #[test]
    fn test_create_user_key_request_serialization() {
        let request = CreateUserKeyRequest {
            private_key:
                "-----BEGIN PGP PRIVATE KEY BLOCK-----.*-----END PGP PRIVATE KEY BLOCK-----"
                    .to_string(),
            primary: 1,
        };
        let expected_json = r#"{"PrivateKey":"-----BEGIN PGP PRIVATE KEY BLOCK-----.*-----END PGP PRIVATE KEY BLOCK-----","Primary":1}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_create_address_key_request_serialization() {
        let request = CreateAddressKeyRequest {
            address_id: "addr_id_1".to_string(),
            private_key: "addr_private_key".to_string(),
            address_forwarding_id: Some("addr_forwarding_id".to_string()),
            primary: 1,
            token: "addr_token".to_string(),
            signature: "addr_signature".to_string(),
            signed_key_list: SignedKeyList {
                data: "data".to_string(),
                signature: "signature".to_string(),
            },
        };
        let expected_json = r#"{"AddressID":"addr_id_1","PrivateKey":"addr_private_key","AddressForwardingID":"addr_forwarding_id","Primary":1,"Token":"addr_token","Signature":"addr_signature","SignedKeyList":{"Data":"data","Signature":"signature"}}"#;

        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(serialized, expected_json);
    }

    #[test]
    fn test_create_user_payload_serialization() {
        let info = ChallengeInfo {
            product_name: "mail-ios".into(),
            device_info: Some(create_device_info_stub()),
            recovery_behavior: Some(Behavior {
                time: vec![123],
                click: 42,
                copy: vec!["copy".into()],
                paste: vec!["paste".into()],
                keydown: vec!["key".into()],
            }),
            username_behavior: None,
        };
        let request = create_user_request_stub(&info);
        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(
            serialized,
            concat!(
                r#"{"Type":1,"Username":"name","Domain":null,"#,
                r#""Auth":{"Version":123,"ModulusID":"mod","Salt":"salt","Verifier":"ver"},"#,
                r#""Email":null,"Phone":null,"Referrer":null,"Payload":{"#,
                r#""mail-ios-v4-challenge-0":{"v":"2.2.0","frame":{"name":"recovery"},"appLang":"lang","timezone":"tz","timezoneOffset":-60,"#,
                r#""deviceName":"model","deviceBrand":"brand","deviceCodename":"code","uuid":"uuid","regionCode":"country","#,
                r#""isJailbreak":false,"preferredContentSize":"scale","storageCapacity":123.0,"isDarkmodeOn":true,"#,
                r#""keyboards":["kb"],"timeRecovery":[123],"clickRecovery":42,"copyRecovery":["copy"],"pasteRecovery":["paste"],"keydownRecovery":["key"]}}}"#,
            )
        );
    }

    #[test]
    fn test_create_anonymous_payload_serialization() {
        let info = ChallengeInfo {
            product_name: "mail-ios".into(),
            device_info: Some(create_device_info_stub()),
            recovery_behavior: None,
            username_behavior: None,
        };
        let request = create_user_request_stub(&info);
        let serialized = serde_json::to_string(&request).expect("Failed to serialize");
        assert_eq!(
            serialized,
            concat!(
                r#"{"Type":1,"Username":"name","Domain":null,"#,
                r#""Auth":{"Version":123,"ModulusID":"mod","Salt":"salt","Verifier":"ver"},"#,
                r#""Email":null,"Phone":null,"Referrer":null,"Payload":{"#,
                r#""mail-ios-v4-challenge-0":{"v":"2.2.0","appLang":"lang","timezone":"tz","timezoneOffset":-60,"#,
                r#""deviceName":"model","deviceBrand":"brand","deviceCodename":"code","uuid":"uuid","regionCode":"country","#,
                r#""isJailbreak":false,"preferredContentSize":"scale","storageCapacity":123.0,"isDarkmodeOn":true,"#,
                r#""keyboards":["kb"]}}}"#,
            )
        );
    }

    fn create_device_info_stub() -> DeviceInfo {
        DeviceInfo {
            language: "lang".into(),
            timezone: "tz".into(),
            timezone_offset: -60,
            model: "model".into(),
            brand: "brand".into(),
            codename: "code".into(),
            uuid: "uuid".into(),
            country: "country".into(),
            rooted: false,
            font_scale: "scale".into(),
            storage: 123.0,
            dark_mode: true,
            keyboards: vec!["kb".into()],
        }
    }

    fn create_user_request_stub(challenge_info: &ChallengeInfo) -> CreateUserRequest {
        CreateUserRequest {
            user_type: CreateUserType::Normal,
            username: "name".into(),
            domain: None,
            auth: AuthInput {
                version: 123,
                modulus_id: "mod".into(),
                salt: "salt".into(),
                verifier: "ver".into(),
            },
            email: None,
            phone: None,
            referrer: None,
            payload: ChallengePayload::new(challenge_info),
        }
    }
}
