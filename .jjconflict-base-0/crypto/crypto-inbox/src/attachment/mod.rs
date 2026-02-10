//! Implements cryptographic helper functions to encrypt and decrypt attachments.

#[allow(clippy::module_name_repetitions)]
mod decrypt;
use std::string::FromUtf8Error;

pub use decrypt::*;

#[allow(clippy::module_name_repetitions)]
mod encrypt;
pub use encrypt::*;

use base64::{Engine as _, prelude::BASE64_STANDARD as BASE_64};
use proton_crypto_account::proton_crypto::{
    CryptoError,
    crypto::{ArmorerSync, PGPProviderSync},
};

use crate::{keys::KeyPacket, string_id};

string_id! {
    /// Encrypted session keys of an attachment.
    KeyPackets
}

impl KeyPackets {
    #[must_use]
    pub fn from_vec(value: Vec<KeyPacket>) -> Self {
        Self(value.into_iter().map(|a| a.0).collect::<String>())
    }

    #[must_use]
    pub fn new_from_bytes(key_packets: &[u8]) -> Self {
        KeyPackets(BASE_64.encode(key_packets))
    }

    pub fn decode(&self) -> Result<Vec<u8>, base64::DecodeError> {
        BASE_64.decode(&self.0)
    }
}

string_id! {
    /// Detached signature over the attachment.
    AttachmentSignature
}

impl AsRef<[u8]> for AttachmentSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

string_id! {
    /// Encrypted detached signature over the attachment.
    AttachmentEncryptedSignature
}

impl AsRef<[u8]> for AttachmentEncryptedSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Errors that can be returned in armoring Attachment metadata.
#[derive(Debug, thiserror::Error)]
pub enum ArmorEncodingError {
    #[error("Failed to to convert to utf-8 string: {0}")]
    Encoding(#[from] FromUtf8Error),
    #[error("Failed to armor PGP type: {0}")]
    Armor(CryptoError),
}

/// A raw binary detached signature over the attachment.
#[derive(Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq, Hash, Clone)]
pub struct BinaryAttachmentSignature(pub Vec<u8>);

impl<T: Into<Vec<u8>>> From<T> for BinaryAttachmentSignature {
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

impl ::std::ops::Deref for BinaryAttachmentSignature {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl BinaryAttachmentSignature {
    /// Armors the signature.
    pub fn armor<P>(&self, pgp: &P) -> Result<AttachmentSignature, ArmorEncodingError>
    where
        P: PGPProviderSync,
    {
        let detached_signature_armored = pgp
            .armorer()
            .armor_signature(&self.0)
            .map_err(ArmorEncodingError::Armor)?;

        let signature = String::from_utf8(detached_signature_armored).map(AttachmentSignature)?;

        Ok(signature)
    }
}

/// A raw encrypted binary detached signature over the attachment.
#[derive(Debug, serde::Deserialize, serde::Serialize, Eq, PartialEq, Hash, Clone)]
pub struct BinaryAttachmentEncryptedSignature(pub Vec<u8>);

impl<T: Into<Vec<u8>>> From<T> for BinaryAttachmentEncryptedSignature {
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

impl ::std::ops::Deref for BinaryAttachmentEncryptedSignature {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl BinaryAttachmentEncryptedSignature {
    /// Armors the encrypted signature.
    pub fn armor<P>(&self, pgp: &P) -> Result<AttachmentEncryptedSignature, ArmorEncodingError>
    where
        P: PGPProviderSync,
    {
        let encrypted_detached_signature_armored = pgp
            .armorer()
            .armor_message(&self.0)
            .map_err(ArmorEncodingError::Armor)?;

        let attachment_encrypted_signature =
            String::from_utf8(encrypted_detached_signature_armored)
                .map(AttachmentEncryptedSignature)?;

        Ok(attachment_encrypted_signature)
    }

    /// Encodes the encrypted signature as a base64 string.
    #[must_use]
    pub fn encode_base64(&self) -> Base64AttachmentEncryptedSignature {
        Base64AttachmentEncryptedSignature(BASE_64.encode(&self.0))
    }
}

string_id! {
    /// A base64 encoded encrypted binary detached signature over the attachment.
    Base64AttachmentEncryptedSignature
}

impl AsRef<[u8]> for Base64AttachmentEncryptedSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}
