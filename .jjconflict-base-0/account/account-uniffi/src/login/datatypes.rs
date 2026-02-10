use muon::{rest::auth::v4::fido2, util::ByteSliceExt};
use proton_core_api::{
    auth::{KeySecret, UserKeySecret},
    store::{MbpMode, UserData},
};
use secrecy::SecretString;
use uniffi::Record;

/// All necessary **unencrypted** data for the migration from legacy version
/// of the app
#[derive(uniffi::Record)]
pub struct MigrationData {
    pub username: String,
    pub display_name: String,
    pub primary_addr: String, // email address
    pub address_signature_enabled: Option<bool>,
    pub mobile_signature: Option<String>,
    pub mobile_signature_enabled: Option<bool>,
    pub key_secret: String, // base64'd
    pub user_id: String,
    pub session_id: String,
    pub password_mode: PasswordMode,

    /// The refresh token. This token must be refreshed before use;
    /// once refreshed, it becomes an access token.
    pub refresh_token: String,
}

impl MigrationData {
    #[must_use]
    pub fn into_parts(self) -> (String, String, UserData, SecretString) {
        let Self {
            username,
            display_name,
            primary_addr,
            key_secret,
            user_id,
            session_id,
            password_mode,
            refresh_token,
            ..
        } = self;

        let password_mode = match password_mode {
            PasswordMode::One => MbpMode::One,
            PasswordMode::Two => MbpMode::Two,
        };

        (
            user_id,
            session_id,
            UserData {
                username,
                display_name,
                primary_addr,
                password_mode,
                key_secret: UserKeySecret(KeySecret::new(key_secret.as_bytes().into())),
            },
            SecretString::new(refresh_token),
        )
    }
}

/// Represents the password mode of an account.
///
/// Note: this is not strictly related to the auth system;
/// it is used to determine whether an account's keys are locked
/// with the primary account password or with a separate password.
#[derive(uniffi::Enum)]
pub enum PasswordMode {
    /// The account has one password.
    One,

    /// The account has two passwords.
    Two,
}

#[derive(Debug, Clone, Record)]
pub struct Fido2RequestFfi {
    pub authentication_options: Fido2AuthenticationOptionsFfi,
    pub client_data: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub signature: Vec<u8>,
    pub credential_id: Vec<u8>,
}

impl From<Fido2RequestFfi> for fido2::Request {
    fn from(dto: Fido2RequestFfi) -> Self {
        fido2::Request {
            authentication_options: dto.authentication_options.into(),
            client_data: dto.client_data.as_b64(),
            authenticator_data: dto.authenticator_data.as_b64(),
            signature: dto.signature.as_b64(),
            credential_id: dto.credential_id,
        }
    }
}

#[derive(Debug, Clone, Record)]
pub struct Fido2ResponseFfi {
    pub authentication_options: Option<Fido2AuthenticationOptionsFfi>,
    pub registered_keys: Vec<RegisteredKeyFfi>,
}

impl From<fido2::Response> for Fido2ResponseFfi {
    fn from(orig: fido2::Response) -> Self {
        Fido2ResponseFfi {
            authentication_options: orig
                .authentication_options
                .map(Fido2AuthenticationOptionsFfi::from),
            registered_keys: orig
                .registered_keys
                .into_iter()
                .map(RegisteredKeyFfi::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Record)]
pub struct RegisteredKeyFfi {
    pub attestation_format: String,
    pub credential_id: Vec<u8>,
    pub name: String,
}

impl From<fido2::RegisteredKey> for RegisteredKeyFfi {
    fn from(orig: fido2::RegisteredKey) -> Self {
        RegisteredKeyFfi {
            attestation_format: orig.attestation_format,
            credential_id: orig.credential_id,
            name: orig.name,
        }
    }
}

#[derive(Debug, Clone, Record)]
pub struct Fido2AuthenticationOptionsFfi {
    pub public_key: Fido2PublicKeyCredentialRequestOptionsFfi,
}

impl From<fido2::AuthenticationOptions> for Fido2AuthenticationOptionsFfi {
    fn from(orig: fido2::AuthenticationOptions) -> Self {
        Fido2AuthenticationOptionsFfi {
            public_key: orig.public_key.into(),
        }
    }
}

impl From<Fido2AuthenticationOptionsFfi> for fido2::AuthenticationOptions {
    fn from(dto: Fido2AuthenticationOptionsFfi) -> Self {
        fido2::AuthenticationOptions {
            public_key: dto.public_key.into(),
        }
    }
}

impl From<Fido2PublicKeyCredentialRequestOptionsFfi> for fido2::PublicKeyCredentialRequestOptions {
    fn from(dto: Fido2PublicKeyCredentialRequestOptionsFfi) -> Self {
        fido2::PublicKeyCredentialRequestOptions {
            challenge: dto.challenge,
            timeout: dto.timeout,
            rp_id: dto.rp_id,
            allow_credentials: dto
                .allow_credentials
                .map(|v| v.into_iter().map(Into::into).collect()),
            user_verification: dto.user_verification,
            extensions: dto.extensions.as_ref().map(|s| {
                fido2::AuthenticationExtensionsClientInputs {
                    app_id: s.app_id.clone(),
                    third_party_payment: s.third_party_payment,
                    uvm: s.uvm,
                }
            }),
        }
    }
}

impl From<Fido2PublicKeyCredentialDescriptorFfi> for fido2::PublicKeyCredentialDescriptor {
    fn from(dto: Fido2PublicKeyCredentialDescriptorFfi) -> Self {
        fido2::PublicKeyCredentialDescriptor {
            credential_type: dto.credential_type,
            id: dto.id,
            transports: dto.transports,
        }
    }
}
#[derive(Debug, Clone, Record)]
pub struct Fido2PublicKeyCredentialRequestOptionsFfi {
    pub challenge: Vec<u8>,
    pub timeout: Option<u64>,
    pub rp_id: Option<String>,
    pub allow_credentials: Option<Vec<Fido2PublicKeyCredentialDescriptorFfi>>,
    pub user_verification: Option<String>,
    pub extensions: Option<Fido2AuthenticationExtensionsClientInputsFfi>,
}

impl From<fido2::PublicKeyCredentialRequestOptions> for Fido2PublicKeyCredentialRequestOptionsFfi {
    fn from(orig: fido2::PublicKeyCredentialRequestOptions) -> Self {
        Fido2PublicKeyCredentialRequestOptionsFfi {
            challenge: orig.challenge,
            timeout: orig.timeout,
            rp_id: orig.rp_id,
            allow_credentials: orig.allow_credentials.map(|creds| {
                creds
                    .into_iter()
                    .map(Fido2PublicKeyCredentialDescriptorFfi::from)
                    .collect()
            }),
            user_verification: orig.user_verification,
            extensions: orig
                .extensions
                .map(Fido2AuthenticationExtensionsClientInputsFfi::from),
        }
    }
}

#[derive(Debug, Clone, Record)]
pub struct Fido2PublicKeyCredentialDescriptorFfi {
    pub credential_type: String,
    pub id: Vec<u8>,
    pub transports: Option<Vec<String>>,
}

impl From<fido2::PublicKeyCredentialDescriptor> for Fido2PublicKeyCredentialDescriptorFfi {
    fn from(orig: fido2::PublicKeyCredentialDescriptor) -> Self {
        Fido2PublicKeyCredentialDescriptorFfi {
            credential_type: orig.credential_type,
            id: orig.id,
            transports: orig.transports,
        }
    }
}

#[derive(Debug, Clone, Record)]
pub struct Fido2AuthenticationExtensionsClientInputsFfi {
    pub app_id: Option<String>,
    pub third_party_payment: Option<bool>,
    pub uvm: Option<bool>,
}

impl From<fido2::AuthenticationExtensionsClientInputs>
    for Fido2AuthenticationExtensionsClientInputsFfi
{
    fn from(orig: fido2::AuthenticationExtensionsClientInputs) -> Self {
        Fido2AuthenticationExtensionsClientInputsFfi {
            app_id: orig.app_id,
            third_party_payment: orig.third_party_payment,
            uvm: orig.uvm,
        }
    }
}
