//! Implements the cryptography package logic to send emails via the API.
use proton_crypto_account::{
    keys::{EmailMimeType, UnlockedAddressKey},
    proton_crypto::{
        crypto::{DataEncoding, Encryptor, EncryptorSync, PGPProviderSync, SessionKeyAlgorithm},
        utils::remove_trailing_spaces,
    },
};

use crate::keys::InboxSessionKey;

use super::MessageError;

const MEGABYTE: usize = 1024 * 1024;

/// All possible mime types of an email package for sending.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PackageMimeType {
    #[default]
    Html,
    Text,
    Multipart,
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
    pub encrypted_body: Vec<u8>, // TODO: Use new type from Rob's MR.
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
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The PGP provider instance from [`proton_crypto_account::proton_crypto`].
    /// * `address_key` - The user's address key with which the body is signed.
    ///
    /// # Errors
    ///
    /// Returns a [`MessageError::Encryption`] error if the encryption fails.
    fn package_body_encrypt<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        address_key: &UnlockedAddressKey<Provider>,
    ) -> Result<EncryptedPackageBody, MessageError> {
        let body = self.package_body_content();
        // TODO: Might want to generate a session key based on the recipients keys
        // The session key is currently hardcoded to AES-256.
        let session_key = pgp_provider
            .session_key_generate(SessionKeyAlgorithm::default())
            .map_err(MessageError::Encryption)?;

        let mut encryptor = pgp_provider
            .new_encryptor()
            .with_session_key_ref(&session_key)
            .with_signing_key(address_key.as_ref())
            .with_utf8();
        // Security trade-off for large mime messages.
        // We use compression on large mime messages with embedded attachments greater than 1MB.
        // Note that compression can lead to side-channels on the message size.
        if self.package_mime_type() == PackageMimeType::Multipart && body.len() > MEGABYTE {
            encryptor = encryptor.with_compression();
        }
        // We need to strip trailing spaces in the content for compatibility.
        let transformed_content =
            remove_trailing_spaces(std::str::from_utf8(self.package_body_content())?);
        let encrypted_body = encryptor
            .encrypt_raw(transformed_content.as_bytes(), DataEncoding::Bytes)
            .map_err(MessageError::Encryption)?;

        Ok(EncryptedPackageBody {
            mime_type: self.package_mime_type(),
            session_key: InboxSessionKey::import_from_pgp_provider(&session_key)?,
            encrypted_body,
        })
    }
}
