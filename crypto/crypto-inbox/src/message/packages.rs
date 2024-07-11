//! Implements the cryptography package logic to send emails via the API.
use proton_crypto_account::{
    keys::UnlockedAddressKey,
    proton_crypto::{
        crypto::{
            AsPublicKeyRef, DataEncoding, Encryptor, EncryptorSync, PGPProviderSync,
            SessionKeyAlgorithm,
        },
        utils::remove_trailing_spaces,
    },
};

use super::{KeyPacket, MessageError, MessageSessionKey};

const MEGABYTE: usize = 1024 * 1024;

/// All possible mime types of an email package for sending.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PackageMimeType {
    #[default]
    Html,
    Text,
    Multipart,
}

/// Represents an encrypted `package` body for sending emails.
///
/// Sending an email with Proton involves creating an encrypted package for each MIME type of the email body.
/// Depending on the recipients' preferences, key packets or the message session key are attached to it.
/// This type provides methods to produce this information.
#[derive(Clone, Eq, PartialEq)]
pub struct EncryptedPackageBody {
    /// The mime type of the package.
    pub mime_type: PackageMimeType,
    /// The message session key the package is encrypted with.
    pub session_key: MessageSessionKey,
    /// The encrypted body of the package in byte format.
    pub encrypted_body: Vec<u8>, // TODO: Use new type from Rob's MR.
}

impl EncryptedPackageBody {
    /// Creates a key packet for the provided recipient public key.
    ///
    /// Encrypts the internal symmetric message session key with the provided public key
    /// using `OpenPGP`. The output is an `OpenPGP` PKESK packet (referred to as key packet in Proton context).
    ///
    /// # Parameters
    ///
    /// * `pgp_provider`  - The pgp provider instance from [`proton_crypto_account::proton_crypto`].
    /// * `recipient_key` - The recipient public key to encrypt the key packet to.
    ///
    /// # Errors
    ///
    /// Returns an [`MessageError::Encryption`] error if the encryption fails or [`MessageError::SessionKeyProblem`] if
    /// there is an issue with the internal session key.
    pub fn key_packet<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        recipient_key: &impl AsPublicKeyRef<Provider::PublicKey>,
    ) -> Result<KeyPacket, MessageError> {
        let session_key = self
            .session_key
            .export_to_pgp_provider(pgp_provider)
            .map_err(MessageError::SessionKeyProblem)?;
        pgp_provider
            .new_encryptor()
            .with_encryption_key(recipient_key.as_public_key())
            .encrypt_session_key(&session_key)
            .map(|key_packet| KeyPacket::new_from_bytes(key_packet.as_ref()))
            .map_err(MessageError::Encryption)
    }

    /// Creates a key packet for each provided recipient public key.
    ///
    /// Encrypts the internal symmetric message session key with the provided public key
    /// using `OpenPGP`. The output is a `OpenPGP` PKESK packet (referred to as key packet in Proton context).
    /// The key packets are returned in order of the provided recipient public keys.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider`  - The pgp provider instance from [`proton_crypto_account::proton_crypto`].
    /// * `recipient_key` - The recipient public key to encrypt the key packet to.
    ///
    /// # Errors
    ///
    /// Returns an [`MessageError::Encryption`] error if the encryption fails or [`MessageError::SessionKeyProblem`] if
    /// there is an issue with the internal session key.
    pub fn key_packets<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        recipient_keys: &[impl AsPublicKeyRef<Provider::PublicKey>],
    ) -> Result<Vec<KeyPacket>, MessageError> {
        let session_key = self
            .session_key
            .export_to_pgp_provider(pgp_provider)
            .map_err(MessageError::SessionKeyProblem)?;
        // Encrypt the session key to each recipient key.
        let mut key_packets = Vec::with_capacity(recipient_keys.len());
        for encryption_key in recipient_keys {
            let key_packet = pgp_provider
                .new_encryptor()
                .with_encryption_key(encryption_key.as_public_key())
                .encrypt_session_key(&session_key)
                .map(|key_packet| KeyPacket::new_from_bytes(key_packet.as_ref()))
                .map_err(MessageError::Encryption)?;
            key_packets.push(key_packet);
        }
        Ok(key_packets)
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
    /// for each recipient address of the package. It also allows extracting the message
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
        // TODO: Might want to generate a session key based one the recipients keys
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
        // We need to strip trailing spaces itn the content for compatibility.
        let transformed_content =
            remove_trailing_spaces(std::str::from_utf8(self.package_body_content())?);
        let encrypted_body = encryptor
            .encrypt_raw(transformed_content.as_bytes(), DataEncoding::Bytes)
            .map_err(MessageError::Encryption)?;

        Ok(EncryptedPackageBody {
            mime_type: self.package_mime_type(),
            session_key: MessageSessionKey::import_from_pgp_provider(&session_key)?,
            encrypted_body,
        })
    }
}
