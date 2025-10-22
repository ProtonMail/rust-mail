use proton_core_api::services::proton::{
    DelinquentState as CoreDelinquentState, Flags as CoreFlags,
    ProductUsedSpace as CoreProductUsedSpace, UserId, UserMnemonicStatus as CoreUserMnemonicStatus,
    UserType as CoreUserType,
};
use proton_crypto_account::keys::{AddressKeys, UserKeys};
use serde::Deserialize;
use serde_aux::field_attributes::deserialize_default_from_null;
use serde_repr::Deserialize_repr;
use serde_with::{BoolFromInt, FromInto, serde_as};

/// The response containing addresses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct GetAddressesResponse {
    /// The list of addresses.
    pub addresses: Vec<Address>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct GetAuthModulusResponse {
    pub modulus: String,

    #[serde(rename = "ModulusID")]
    pub modulus_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct GetPasswordPoliciesResponse {
    pub code: ResponseCode,
    pub password_policies: Vec<PasswordPolicyResponse>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PasswordPolicyResponse {
    /// The name of the password policy. This serves as identifier.
    pub policy_name: String,

    /// The state of the password policy. Disabled policies are not returned.
    pub state: PasswordPolicyState,

    /// The requirement message. This is a relatively short string informing the user how to fulfill the policy.
    pub requirement_message: String,

    /// The error message. This string is intended to be displayed to the user when they try to proceed with a password that does not respect the policy.
    pub error_message: String,

    /// The regex. It should be applied to the password. If it returns true, the policy passed.
    pub regex: String,

    /// Whether the policy should be hidden when the password respects it. In other words it should only appear when violated.
    pub hide_if_valid: bool,
}

#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PasswordPolicyState {
    Disabled = 0,
    Enabled = 1,
    Optional = 2,
}

/// This enum defines different categories of addresses with assigned integer values.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum AddressType {
    /// An initial type of address.
    Original = 1,

    /// A secondary or alternate address.
    Alias = 2,

    /// A tailored or unique address.
    Custom = 3,

    /// An enhanced or special address.
    Premium = 4,

    /// An address from an outside source.
    External = 5,
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum UserType {
    /// TODO: Document this variant.
    Proton = 1,

    /// TODO: Document this variant.
    Managed = 2,

    /// TODO: Document this variant.
    External = 3,

    /// Credentialless user
    CredentialLess = 4,

    Unknown(u8),
}

impl From<u8> for UserType {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Proton,
            2 => Self::Managed,
            3 => Self::External,
            4 => Self::CredentialLess,
            v => Self::Unknown(v),
        }
    }
}

impl From<UserType> for u8 {
    fn from(value: UserType) -> Self {
        match value {
            UserType::Proton => 1,
            UserType::Managed => 2,
            UserType::External => 3,
            UserType::CredentialLess => 4,
            UserType::Unknown(v) => v,
        }
    }
}

/// Represents the delinquent state of the user.
///
/// This enum indicates the payment status of the user's account.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize_repr, Eq)]
#[serde(rename_all = "PascalCase")]
#[repr(u32)]
pub enum DelinquentState {
    /// The user's account is fully paid.
    Paid = 0,
    /// The user's account is available but not yet paid.
    Available = 1,
    /// The user's account has an overdue payment.
    Overdue = 2,
    /// The user's account is delinquent due to unpaid dues.
    Delinquent = 3,
    /// The user's payment has not been received.
    NotReceived = 4,
}

// TODO move get_users api call from core to account, and use account's own
// User type for the response, so we can get rid of this conversion
impl From<CoreDelinquentState> for DelinquentState {
    fn from(value: CoreDelinquentState) -> Self {
        match value {
            CoreDelinquentState::Paid => DelinquentState::Paid,
            CoreDelinquentState::Available => DelinquentState::Available,
            CoreDelinquentState::Overdue => DelinquentState::Overdue,
            CoreDelinquentState::Delinquent => DelinquentState::Delinquent,
            CoreDelinquentState::NotReceived => DelinquentState::NotReceived,
        }
    }
}

impl From<DelinquentState> for CoreDelinquentState {
    fn from(value: DelinquentState) -> Self {
        match value {
            DelinquentState::Paid => CoreDelinquentState::Paid,
            DelinquentState::Available => CoreDelinquentState::Available,
            DelinquentState::Overdue => CoreDelinquentState::Overdue,
            DelinquentState::Delinquent => CoreDelinquentState::Delinquent,
            DelinquentState::NotReceived => CoreDelinquentState::NotReceived,
        }
    }
}

impl From<UserMnemonicStatus> for CoreUserMnemonicStatus {
    fn from(value: UserMnemonicStatus) -> Self {
        match value {
            UserMnemonicStatus::Disabled => CoreUserMnemonicStatus::Disabled,
            UserMnemonicStatus::EnabledButNotSet => CoreUserMnemonicStatus::EnabledButNotSet,
            UserMnemonicStatus::EnabledNeedsReactivation => {
                CoreUserMnemonicStatus::EnabledNeedsReactivation
            }
            UserMnemonicStatus::EnabledAndSet => CoreUserMnemonicStatus::EnabledAndSet,
            UserMnemonicStatus::Unknown => CoreUserMnemonicStatus::Unknown,
        }
    }
}

impl From<ProductUsedSpace> for CoreProductUsedSpace {
    fn from(val: ProductUsedSpace) -> Self {
        Self {
            calendar: val.calendar,
            contact: val.contact,
            drive: val.drive,
            mail: val.mail,
            pass: val.pass,
        }
    }
}

impl From<Flags> for CoreFlags {
    fn from(flags: Flags) -> Self {
        Self {
            has_temporary_password: flags.has_temporary_password,
            no_login: flags.no_login,
            no_proton_address: flags.no_proton_address,
            onboard_checklist_storage_granted: flags.onboard_checklist_storage_granted,
            protected: flags.protected,
            recovery_attempt: flags.recovery_attempt,
            sso: flags.sso,
            test_account: flags.test_account,
            has_a_byoe_address: flags.has_a_byoe_address,
        }
    }
}

/// The response code indicating the status of the request.
/// A value of 1000 typically indicates success.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SimpleResponse {
    code: ResponseCode,
}

/// The response code indicating the status of the request.
/// A value of 1000 typically indicates success.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ResponseCode(i32);

/// Represents the response to a request for setting up a new non-subscriber user address.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PostAddressesSetupResponse {
    /// The response code indicating the success or failure of the request (e.g., 1000 for success).
    pub code: ResponseCode,

    /// The details of the newly created address.
    pub address: PostAddressesSetupResponseAddress,
}

/// Represents the response to a request for creating a user key.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CreateUserKeyResponse {
    /// The response code indicating the success or failure of the request (e.g., 1000 for success).
    pub code: ResponseCode,

    /// The unique identifier of the created key.
    #[serde(rename = "KeyID")]
    pub key_id: String,
}

/// Represents the response to a request for creating an address key.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CreateAddressKeyResponse {
    /// The response code indicating the success or failure of the request (e.g., 1000 for success).
    pub code: ResponseCode,

    /// The details of the created key.
    pub key: AddressKey,
}

/// Represents an address key returned in the response.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AddressKey {
    /// The unique identifier of the key.
    #[serde(rename = "ID")]
    pub id: String,

    /// The version of the key.
    pub version: u32,

    /// The flags associated with the key.
    pub flags: u32,

    /// The private key.
    pub private_key: String,

    /// The token associated with the key (can be null).
    pub token: Option<String>,

    /// The signature of the key (can be null).
    pub signature: Option<String>,

    /// The fingerprint of the key.
    pub fingerprint: String,

    /// List of fingerprints.
    pub fingerprints: Vec<String>,

    /// The activation status (can be null).
    pub activation: Option<String>,

    /// Indicates if this is the primary key.
    pub primary: u8,

    /// Indicates if the key is active.
    pub active: u8,
}

/// Represents the details of a newly created address for a non-subscriber user.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PostAddressesSetupResponseAddress {
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

/// Represents the status of an address.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum AddressStatus {
    /// The address is disabled.
    Disabled = 0,

    /// The address is enabled.
    Enabled = 1,

    /// The address is in the process of being deleted.
    Deleting = 2,
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

/// Represents the response to a request creating a new user.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateUserResponse {
    /// The response code indicating the success or failure of the request (e.g., 1000 for success).
    pub code: ResponseCode,

    /// The details of the newly created user.
    pub user: User,
}

/// Represents an API user
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct User {
    #[serde(rename = "ID")]
    pub id: UserId,

    pub create_time: u64,

    pub credit: i64,

    pub currency: String,

    /// Indicates the delinquency status of the user's account.
    pub delinquent: DelinquentState,

    pub display_name: Option<String>,

    pub email: String,

    pub flags: Flags,

    pub keys: UserKeys,

    pub max_space: i64,

    pub max_upload: i64,

    pub mnemonic_status: UserMnemonicStatus,

    pub name: Option<String>,

    pub private: u32,

    pub product_used_space: ProductUsedSpace,

    pub role: u32,

    pub services: u32,

    pub subscribed: u32,

    #[serde_as(as = "BoolFromInt")]
    pub to_migrate: bool,

    pub used_space: i64,

    #[serde(rename = "Type")]
    #[serde_as(as = "FromInto<u8>")]
    pub user_type: UserType,
}

impl From<User> for proton_core_api::services::proton::User {
    fn from(val: User) -> Self {
        Self {
            id: val.id,
            create_time: val.create_time,
            credit: val.credit,
            currency: val.currency,
            delinquent: val.delinquent.into(),
            display_name: val.display_name,
            email: val.email,
            flags: val.flags.into(),
            keys: val.keys,
            max_space: val.max_space,
            max_upload: val.max_upload,
            mnemonic_status: val.mnemonic_status.into(),
            name: val.name,
            private: val.private != 0,
            product_used_space: val.product_used_space.into(),
            role: val.role.into(),
            services: val.services,
            subscribed: val.subscribed,
            to_migrate: val.to_migrate,
            used_space: val.used_space,
            user_type: CoreUserType::from(Into::<u8>::into(val.user_type)),
        }
    }
}

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Deserialize_repr, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum UserMnemonicStatus {
    /// TODO: Document this variant.
    Disabled = 0,

    /// TODO: Document this variant.
    EnabledButNotSet = 1,

    /// TODO: Document this variant.
    EnabledNeedsReactivation = 2,

    /// TODO: Document this variant.
    EnabledAndSet = 3,

    /// TODO: Document this variant.
    Unknown = 4,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ProductUsedSpace {
    /// TODO: Document this field.
    pub calendar: i64,

    /// TODO: Document this field.
    pub contact: i64,

    /// TODO: Document this field.
    pub drive: i64,

    /// TODO: Document this field.
    pub mail: i64,

    /// TODO: Document this field.
    pub pass: i64,
}

/// The address of a user (copied from `proton-api-core`)
#[serde_as]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    // TODO(ET-3061): this type should be `AddressId`
    // (see `core/core-api/src/services/proton/core/common.rs`),
    // but those types should be put into a slimmer common crate,
    // which defines only common types and DTO-s.
    #[serde(rename = "ID")]
    pub id: String,

    /// TODO: Document this field.
    #[serde(rename = "Type")]
    pub address_type: AddressType,

    /// TODO: Document this field.
    pub catch_all: bool,

    /// TODO: Document this field.
    pub display_name: Option<String>,

    /// TODO: Document this field.
    #[serde(rename = "DomainID")]
    pub domain_id: Option<String>,

    /// TODO: Document this field.
    pub email: String,

    /// TODO: Document this field.
    pub keys: AddressKeys,

    /// TODO: Document this field.
    pub order: u32,

    /// TODO: Document this field.
    #[serde(rename = "ProtonMX")]
    pub proton_mx: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub receive: bool,

    /// TODO: Document this field.
    #[serde_as(as = "BoolFromInt")]
    pub send: bool,

    /// TODO: Document this field.
    pub signature: Option<String>,

    /// TODO: Document this field.
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub signed_key_list: AddressSignedKeyList,

    /// TODO: Document this field.
    pub status: AddressStatus,
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AddressSignedKeyList {
    /// TODO: Document this field.
    pub data: Option<String>,

    /// TODO: Document this field.
    #[serde(rename = "ExpectedMinEpochID")]
    pub expected_min_epoch_id: Option<u64>,

    /// TODO: Document this field.
    #[serde(rename = "MaxEpochID")]
    pub max_epoch_id: Option<u64>,

    /// TODO: Document this field.
    #[serde(rename = "MinEpochID")]
    pub min_epoch_id: Option<u64>,

    /// TODO: Document this field.
    pub obsolescence_token: Option<String>,

    /// TODO: Document this field.
    pub revision: u64,

    /// TODO: Document this field.
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Flags {
    /// TODO: Document this field.
    #[serde(rename = "has-temporary-password")]
    pub has_temporary_password: bool,

    /// Whether the user has a BYOE address.
    #[serde(rename = "has-a-byoe-address")]
    #[serde(default)]
    pub has_a_byoe_address: bool,

    /// TODO: Document this field.
    #[serde(rename = "no-login")]
    pub no_login: bool,

    /// TODO: Document this field.
    #[serde(rename = "no-proton-address")]
    pub no_proton_address: bool,

    /// TODO: Document this field.
    #[serde(rename = "onboard-checklist-storage-granted")]
    pub onboard_checklist_storage_granted: bool,

    /// TODO: Document this field.
    pub protected: bool,

    /// TODO: Document this field.
    #[serde(rename = "recovery-attempt")]
    pub recovery_attempt: bool,

    /// TODO: Document this field.
    pub sso: bool,

    /// TODO: Document this field.
    #[serde(rename = "test-account")]
    pub test_account: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize_repr)]
#[repr(u8)]
pub enum TwoFaEnabled {
    Disabled = 0,
    Otp = 1,
    Fido2 = 2,
    Both = 3,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RegisteredKey {
    pub attestation_format: String,
    #[serde(rename = "CredentialID")]
    pub credential_id: Vec<Option<i32>>,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Fido2 {
    pub authentication_options: serde_json::Value,
    pub registered_keys: Vec<RegisteredKey>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct TwoFaInfo {
    pub enabled: TwoFaEnabled,
    #[serde(rename = "FIDO2")]
    pub fido2: Fido2,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AuthResponse {
    pub code: ResponseCode,

    /// Session unique ID
    #[serde(rename = "UID")]
    pub uid: String,
    #[serde(rename = "UserID")]
    pub user_id: String,
    #[serde(rename = "EventID")]
    pub event_id: String,
    pub server_proof: String,
    /// only if the session is not in cookie mode
    pub token_type: String,
    pub access_token: String,
    pub refresh_token: String,
    #[serde(rename = "LocalID")]
    pub local_id: i32,
    pub scopes: Vec<String>,
    pub password_mode: i32,

    /// If 1 the user should be prompted to enter a new password on login
    pub temporary_password: i32,
    #[serde(rename = "2FA")]
    pub two_fa: TwoFaInfo,
}

/// Response for `PUT /settings/password` endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PutSettingsPasswordResponse {
    /// Standard response code (1000 for success).
    pub code: u32,
}

/// Response for `PUT /keys/private` endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PutKeysPrivateResponse {
    /// Standard response code (1000 for success).
    pub code: u32,
}

/// Response for password change authentication endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PutUsersPasswordResponse {
    /// Standard response code (1000 for success).
    pub code: u32,

    /// Base64-encoded server proof.
    pub server_proof: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_crypto_account::keys::{
        ArmoredPrivateKey, EncryptedKeyToken, KeyId, KeyTokenSignature, LockedKey, UserKeys,
    };
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

        let response: PostAddressesSetupResponse =
            serde_json::from_str(json).expect("Failed to deserialize JSON");

        let expected = PostAddressesSetupResponse {
            code: ResponseCode(1000),
            address: PostAddressesSetupResponseAddress {
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
                    has_a_byoe_address: false,
                },
            },
        };

        let deserialized: SetupKeysResponse =
            serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(deserialized, expected);
    }

    #[test]
    fn test_get_auth_deserialization() {
        let json = r#"
        {
          "Code": 1000,
          "UID": "6f3c4f52cf499c2066e6c5669a293177c1f43755",
          "UserID": "-Bpgivr5H2qGDRiUQ4-7gm5YLf215MEgZCdzOtLW5psxgB8oNc8OnoFRykab4Z23EGEW1ka3GtQPF9xwx9-VUA==",
          "EventID": "ACXDmTaBub14w==",
          "ServerProof": "<base64_encoded_proof>",
          "TokenType": "Bearer",
          "AccessToken": "hnnamrzvsgdbxvx74rjadbovyjy63vz4",
          "RefreshToken": "wfih0367aa7dc0359bf5c42d15a93e6c",
          "ExpiresIn": 360000,
          "LocalID": 0,
          "Scopes": [
            "full"
          ],
          "Scope": "full other_scopes",
          "PasswordMode": 2,
          "TemporaryPassword": 0,
          "2FA": {
            "Enabled": 3,
            "FIDO2": {
              "AuthenticationOptions": {},
              "RegisteredKeys": [
                {
                  "AttestationFormat": "fido2-u2f",
                  "CredentialID": [
                    null
                  ],
                  "Name": "My security key"
                }
              ]
            }
          }
        }
        "#;

        let response: AuthResponse =
            serde_json::from_str(json).expect("Failed to deserialize JSON");

        let expected = AuthResponse {
            code: ResponseCode(1000),
            uid: String::from("6f3c4f52cf499c2066e6c5669a293177c1f43755"),
            user_id: String::from(
                "-Bpgivr5H2qGDRiUQ4-7gm5YLf215MEgZCdzOtLW5psxgB8oNc8OnoFRykab4Z23EGEW1ka3GtQPF9xwx9-VUA==",
            ),
            event_id: String::from("ACXDmTaBub14w=="),
            server_proof: String::from("<base64_encoded_proof>"),
            token_type: String::from("Bearer"),
            access_token: String::from("hnnamrzvsgdbxvx74rjadbovyjy63vz4"),
            refresh_token: String::from("wfih0367aa7dc0359bf5c42d15a93e6c"),
            local_id: 0,
            scopes: vec![String::from("full")],
            password_mode: 2,
            temporary_password: 0,
            two_fa: TwoFaInfo {
                enabled: TwoFaEnabled::Both,
                fido2: Fido2 {
                    authentication_options: serde_json::Value::Object(serde_json::Map::default()),
                    registered_keys: vec![RegisteredKey {
                        attestation_format: String::from("fido2-u2f"),
                        credential_id: vec![None],
                        name: String::from("My security key"),
                    }],
                },
            },
        };

        assert_eq!(response, expected);
    }

    #[test]
    fn test_create_user_key_response_deserialization() {
        let json = r#"
        {
            "Code": 1000,
            "KeyID": "G1MbEt3Ep5P_EWz8WbHVAOl_6h=="
        }
        "#;

        let response: CreateUserKeyResponse =
            serde_json::from_str(json).expect("Failed to deserialize JSON");

        let expected = CreateUserKeyResponse {
            code: ResponseCode(1000),
            key_id: "G1MbEt3Ep5P_EWz8WbHVAOl_6h==".to_string(),
        };

        assert_eq!(response, expected);
    }

    #[test]
    fn test_create_address_key_response_deserialization() {
        let json = r#"
        {
            "Code": 1000,
            "Key": {
                "ID": "G1MbEt3Ep5P_EWz8WbHVAOl_6h==",
                "Version": 3,
                "Flags": 3,
                "PrivateKey": "-----BEGIN PGP PRIVATE KEY BLOCK-----.*-----END PGP PRIVATE KEY BLOCK-----",
                "Token": "-----BEGIN PGP MESSAGE-----.*-----END PGP MESSAGE-----",
                "Signature": "-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----",
                "Fingerprint": "c93f767df53b0ca8395cfde90483475164ec6353",
                "Fingerprints": [
                    "c93f767df53b0ca8395cfde90483475164ec6353"
                ],
                "Activation": null,
                "Primary": 1,
                "Active": 1
            }
        }
        "#;

        let response: CreateAddressKeyResponse =
            serde_json::from_str(json).expect("Failed to deserialize JSON");

        let expected = CreateAddressKeyResponse {
            code: ResponseCode(1000),
            key: AddressKey {
                id: "G1MbEt3Ep5P_EWz8WbHVAOl_6h==".to_string(),
                version: 3,
                flags: 3,
                private_key:
                    "-----BEGIN PGP PRIVATE KEY BLOCK-----.*-----END PGP PRIVATE KEY BLOCK-----"
                        .to_string(),
                token: Some("-----BEGIN PGP MESSAGE-----.*-----END PGP MESSAGE-----".to_string()),
                signature: Some(
                    "-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----".to_string(),
                ),
                fingerprint: "c93f767df53b0ca8395cfde90483475164ec6353".to_string(),
                fingerprints: vec!["c93f767df53b0ca8395cfde90483475164ec6353".to_string()],
                activation: None,
                primary: 1,
                active: 1,
            },
        };

        assert_eq!(response, expected);
    }

    #[test]
    fn test_password_policies() {
        let json = r#"
            {
                "Code": 1000,
                "PasswordPolicies": [
                    {
                        "PolicyName": "DisallowSequences",
                        "State": 1,
                        "RequirementMessage": "No sequences (not 123 or abc)",
                        "ErrorMessage": "Password must not contain a sequence (not 123 or abc)",
                        "Regex": "^(?:(?!(.)\\1{2}|012|123|234|345|456|567|678|789|890|210|321|432|543|654|765|876|987|098|abc|bcd|cde|def|efg|fgh|ghi|hij|ijk|jkl|klm|lmn|mno|nop|opq|pqr|qrs|rst|stu|tuv|uvw|vwx|wxy|xyz).)*$",
                        "HideIfValid": true
                    },
                    {
                        "PolicyName": "DisallowCommonPasswords",
                        "State": 2,
                        "RequirementMessage": "Something not too common",
                        "ErrorMessage": "Password shouldn't be too common or too predictable",
                        "Regex": "^(?!(protonmail|protonvpn|protondrive|protonpass|)$).*$",
                        "HideIfValid": false
                    }
                ]
            }
        "#;

        let response: GetPasswordPoliciesResponse =
            serde_json::from_str(json).expect("Failed to deserialize JSON");

        let expected = GetPasswordPoliciesResponse {
            code: ResponseCode(1000),
            password_policies: vec![
                PasswordPolicyResponse {
                    policy_name: String::from("DisallowSequences"),
                    state: PasswordPolicyState::Enabled,
                    requirement_message: String::from("No sequences (not 123 or abc)"),
                    error_message: String::from(
                        "Password must not contain a sequence (not 123 or abc)",
                    ),
                    regex: String::from(
                        r"^(?:(?!(.)\1{2}|012|123|234|345|456|567|678|789|890|210|321|432|543|654|765|876|987|098|abc|bcd|cde|def|efg|fgh|ghi|hij|ijk|jkl|klm|lmn|mno|nop|opq|pqr|qrs|rst|stu|tuv|uvw|vwx|wxy|xyz).)*$",
                    ),
                    hide_if_valid: true,
                },
                PasswordPolicyResponse {
                    policy_name: String::from("DisallowCommonPasswords"),
                    state: PasswordPolicyState::Optional,
                    requirement_message: String::from("Something not too common"),
                    error_message: String::from(
                        "Password shouldn't be too common or too predictable",
                    ),
                    regex: String::from(r"^(?!(protonmail|protonvpn|protondrive|protonpass|)$).*$"),
                    hide_if_valid: false,
                },
            ],
        };

        assert_eq!(response, expected);
    }
}
