//! Implements the cryptography package logic to send emails via the API.
use std::{fmt, str::FromStr};

use proton_crypto_account::{
    keys::{EmailMimeType, PrimaryUnlockedAddressKey},
    proton_crypto::{
        crypto::{DataEncoding, Encryptor, EncryptorSync, PGPProviderSync, SessionKeyAlgorithm},
        utils::remove_trailing_spaces,
    },
};

use crate::keys::InboxSessionKey;

use super::{EncryptedMessageBody, MessageError, SessionKeyAndDataPacketsExtractable};

const MEGABYTE: usize = 1024 * 1024;

/// All possible mime types of an email package for sending.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PackageMimeType {
    #[default]
    Html,
    Text,
    Multipart,
}

impl fmt::Display for PackageMimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            PackageMimeType::Text => "text/plain",
            PackageMimeType::Html => "text/html",
            PackageMimeType::Multipart => "multipart/mixed",
        };
        write!(f, "{text}")
    }
}

impl FromStr for PackageMimeType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text/plain" => Ok(PackageMimeType::Text),
            "text/html" => Ok(PackageMimeType::Html),
            "multipart/mixed" => Ok(PackageMimeType::Multipart),
            _ => Err("Unknown MIME type"),
        }
    }
}

impl From<EmailMimeType> for PackageMimeType {
    fn from(value: EmailMimeType) -> Self {
        match value {
            EmailMimeType::Html => Self::Html,
            EmailMimeType::Text => Self::Text,
        }
    }
}

/// Represents an encrypted `package` body for sending emails.
///
/// Sending an email with Proton involves creating an encrypted package for each MIME type of the email body.
/// Depending on the recipients' preferences, either key packets or the message session key are attached to it.
/// This type provides methods to generate this information.
#[derive(Clone, Eq, PartialEq)]
pub struct EncryptedPackageBody {
    /// The mime type of the package.
    pub mime_type: PackageMimeType,

    /// The message session key with which the package is encrypted.
    ///
    /// Provides methods to re-encrypt the message for new recipients
    /// or to expose the session key.
    pub session_key: InboxSessionKey,

    /// The encrypted body of the package in byte format.
    pub encrypted_body: EncryptedMessageBody,
}

impl EncryptedPackageBody {
    /// Creates an [`EncryptedPackageBody`] from an existing draft.
    ///
    /// This function attempts to extract the session key from the provided draft using the given `decryption_keys`.
    /// If successful, it constructs an [`EncryptedPackageBody`] with the specified `mime_type`.
    pub fn new_with_draft<Provider: PGPProviderSync, Draft: SessionKeyAndDataPacketsExtractable>(
        provider: &Provider,
        draft: &Draft,
        mime_type: PackageMimeType,
        decryption_keys: &[impl AsRef<Provider::PrivateKey>],
    ) -> Result<Self, MessageError> {
        let (session_key, encrypted_body) =
            draft.extract_session_key_and_data_packets(provider, decryption_keys)?;
        Ok(Self {
            mime_type,
            session_key,
            encrypted_body,
        })
    }
}

pub trait EncryptablePackage {
    /// Returns the mime type of the content.
    ///
    /// See [`PackageBodyType`] for possible options.
    fn package_mime_type(&self) -> PackageMimeType;

    /// Returns the plain text content of the body as a byte slice.
    fn package_body_content(&self) -> &[u8];

    /// Signs and encrypts the package body, preparing it for sending.
    ///
    /// The returned [`EncryptedPackageBody`] can be used to create individual key packets
    /// for each recipient address of the package. It also allows extraction of the message
    /// session key to enable the server to decrypt the content for cleartext recipients.
    fn package_body_encrypt<P>(
        &self,
        pgp: &P,
        address_key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    ) -> Result<EncryptedPackageBody, MessageError>
    where
        P: PGPProviderSync,
    {
        package_body_encrypt(
            pgp,
            address_key,
            self.package_mime_type(),
            self.package_body_content(),
        )
    }
}

/// Signs and encrypts the package body, preparing it for sending.
///
/// The returned [`EncryptedPackageBody`] can be used to create individual key packets
/// for each recipient address of the package. It also allows extraction of the message
/// session key to enable the server to decrypt the content for cleartext recipients.
pub fn package_body_encrypt<P>(
    pgp: &P,
    address_key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
    mime_type: PackageMimeType,
    body: &[u8],
) -> Result<EncryptedPackageBody, MessageError>
where
    P: PGPProviderSync,
{
    // TODO: Might want to generate a session key based on the recipients keys
    // The session key is currently hardcoded to AES-256.
    let session_key = pgp
        .session_key_generate(SessionKeyAlgorithm::default())
        .map_err(MessageError::Encryption)?;

    let mut encryptor = pgp
        .new_encryptor()
        .with_session_key_ref(&session_key)
        .with_signing_keys(address_key.for_signing())
        .with_utf8();

    // Security trade-off for large mime messages.
    // We use compression on large mime messages with embedded attachments greater than 1MB.
    // Note that compression can lead to side-channels on the message size.
    if mime_type == PackageMimeType::Multipart && body.len() > MEGABYTE {
        encryptor = encryptor.with_compression();
    }

    // We need to strip trailing spaces in the content for compatibility.
    let transformed_content = remove_trailing_spaces(std::str::from_utf8(body)?);

    let encrypted_body = encryptor
        .encrypt_raw(transformed_content.as_bytes(), DataEncoding::Bytes)
        .map_err(MessageError::Encryption)?;

    Ok(EncryptedPackageBody {
        mime_type,
        session_key: InboxSessionKey::import_from_pgp_provider(&session_key)?,
        encrypted_body: EncryptedMessageBody::from(encrypted_body),
    })
}
