use serde::{Deserialize, Serialize};

/// TFA Fido details for a user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthenticationOptions {
    /// Public key Fido details.
    #[serde(rename = "publicKey")]
    pub public_key: PublicKeyCredentialRequestOptions,
}

/// TFA public key FIDO details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicKeyCredentialRequestOptions {
    /// Public key challenge details.
    pub challenge: Vec<u8>,
    /// Timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// RpId.
    #[serde(rename = "rpId", skip_serializing_if = "Option::is_none")]
    pub rp_id: Option<String>,
    /// If credentials are allowed.
    #[serde(rename = "allowCredentials", skip_serializing_if = "Option::is_none")]
    pub allow_credentials: Option<Vec<PublicKeyCredentialDescriptor>>,
    /// Public Key user verification.
    #[serde(rename = "userVerification", skip_serializing_if = "Option::is_none")]
    pub user_verification: Option<String>,
    /// Public Key extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<AuthenticationExtensionsClientInputs>,
}

/// Public Key credential descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicKeyCredentialDescriptor {
    /// Type.
    #[serde(rename = "type")]
    pub credential_type: String,
    /// Id.
    pub id: Vec<u8>,
    /// Transport options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transports: Option<Vec<String>>,
}

/// Auth extensions.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct AuthenticationExtensionsClientInputs {
    /// App id.
    #[serde(rename = "appId", skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// Third party payment.
    #[serde(rename = "thirdPartyPayment", skip_serializing_if = "Option::is_none")]
    pub third_party_payment: Option<bool>,
    /// Uvm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uvm: Option<bool>,
}

impl PublicKeyCredentialRequestOptions {
    /// Checks if hs extension.
    pub fn has_extensions(&self) -> bool {
        match &self.extensions {
            Some(ext) => {
                ext.app_id.is_some() || ext.third_party_payment.is_some() || ext.uvm.is_some()
            }
            None => false,
        }
    }
}

/// The request for Fido2 to a `POST /auth/v4/2fa`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Request {
    /// Corresponds to CredentialRequestOptions Dictionary Extension (https://www.w3.org/TR/webauthn-2/#sctn-credentialrequestoptions-extension).
    pub authentication_options: AuthenticationOptions,

    /// PublicKeyCredential Client data json. Base64 string.
    pub client_data: String,

    /// PublicKeyCredential Authenticator data. Base64 string.
    pub authenticator_data: String,

    /// PublicKeyCredential Signature. Base64 string.
    pub signature: String,

    /// PublicKeyCredential Credential id.
    #[serde(rename = "CredentialID")]
    pub credential_id: Vec<u8>,
}

/// TFA Fido details for a user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct Response {
    /// Refer to the definition of PublicKeyCredentialRequestOptions in the
    /// WebAuthn spec. Binary data is encoded as Uint8Array.
    pub authentication_options: Option<AuthenticationOptions>,

    /// A collection of registered FIDO keys associated with the user.
    pub registered_keys: Vec<RegisteredKey>,
}

/// Registered FIDO key for a user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct RegisteredKey {
    /// The attestation format used for the credential, as defined in the
    /// WebAuthn specification.
    pub attestation_format: String,

    /// The unique identifier for the credential, encoded as a vector of bytes.
    #[serde(rename = "CredentialID")]
    pub credential_id: Vec<u8>,

    /// A user-friendly name or label for the registered key.
    pub name: String,
}
