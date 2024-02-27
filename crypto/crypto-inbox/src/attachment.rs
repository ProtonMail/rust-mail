use base64::prelude::*;
use std::{
    fmt::{Display, Formatter},
    io,
};

use proton_crypto::crypto::{
    DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerificationStatus, VerifiedData,
    VerifiedDataReader,
};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("Could not decode key packets: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Failed to decrypt key packets to session key with the decryption keys: {0}")]
    SessionKeyDecryption(Box<dyn std::error::Error>),
    #[error("Failed to decrypt attachment with the extracted session key: {0}")]
    AttachmentDecryption(Box<dyn std::error::Error>),
    #[error("Failed to decrypt and write to the output writer: {0}")]
    AttachmentDecryptionWrite(std::io::Error),
}

/// Represent an attachments's API key packets.
#[derive(Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct KeyPackets(String);

impl Display for KeyPackets {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Into<String>> From<T> for KeyPackets {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for KeyPackets {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl KeyPackets {
    pub fn decode(&self) -> Result<Vec<u8>, AttachmentError> {
        let b64 = base64::engine::general_purpose::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::general_purpose::PAD,
        );
        b64.decode(&self.0).map_err(|err| err.into())
    }
}

/// Represent an attachments's API detached signature.
#[derive(Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct AttachmentSignature(String);

impl Display for AttachmentSignature {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Into<String>> From<T> for AttachmentSignature {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for AttachmentSignature {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for AttachmentSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Represent an attachments's API encrypted detached signature.
#[derive(Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct EncryptedAttachmentSignature(String);

impl Display for EncryptedAttachmentSignature {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Into<String>> From<T> for EncryptedAttachmentSignature {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for EncryptedAttachmentSignature {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<[u8]> for EncryptedAttachmentSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

pub struct DecryptedAttachment<T: VerifiedData>(T);

impl<T: VerifiedData> AsRef<[u8]> for DecryptedAttachment<T> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl<T: VerifiedData> DecryptedAttachment<T> {
    pub fn signature_verification_status(&self) -> AttachmentVerification {
        let status = self
            .0
            .get_verification_status()
            .unwrap_or(VerificationStatus::NotSigned(
                "No signature provided".into(),
            ));
        AttachmentVerification { status }
    }
}

pub struct AttachmentVerification {
    // TODO: Add more info here
    pub status: VerificationStatus,
}

#[derive(Deserialize, Debug)]
pub struct Attachment {
    #[serde(rename = "KeyPackets")]
    pub key_packets: KeyPackets,
    #[serde(rename = "KeySalt")]
    pub signature: Option<AttachmentSignature>,
    #[serde(rename = "EncSignature")]
    pub enc_signature: Option<EncryptedAttachmentSignature>,
}

pub trait AttachmentCrypto {
    /// Decrypts an attachment based on its metadata.
    ///
    /// Decrypts the attachment session key from the key packets with the `decryption_keys`,
    /// then used the session key to decrypt the `attachment_data`, and tries to verify one
    /// of the signatures signature/enc_signature if present with the `verification_keys`.
    /// The signature verification result can be accessed trough the returned `DecryptedAttachment`.
    fn decrypt_attachment<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        decryption_keys: impl AsRef<[<T>::PrivateKey]>,
        verification_keys: impl AsRef<[<T>::PublicKey]>,
        attachment_data: impl AsRef<[u8]>,
    ) -> Result<DecryptedAttachment<T::VerifiedData>, AttachmentError>;
    /// Decrypts an attachment from an attachment data reader.
    ///
    /// Decrypts the attachment session key from the key packets with the `decryption_keys`,
    /// then used the session key to decrypt the `attachment_data`, and tries to verify one
    /// of the signatures signature/enc_signature if present with the `verification_keys`.
    /// The signature verification result is returned while the attachment data is written to the `output_writer`.
    fn decrypt_attachment_from_reader<T: PGPProviderSync, R: io::Read, W: io::Write>(
        &self,
        pgp_provider: &T,
        decryption_keys: impl AsRef<[<T>::PrivateKey]>,
        _verification_keys: impl AsRef<[<T>::PublicKey]>,
        attachment_data: R,
        output_writer: &mut W,
    ) -> Result<AttachmentVerification, AttachmentError>;
}

impl AttachmentCrypto for Attachment {
    fn decrypt_attachment<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        decryption_keys: impl AsRef<[<T>::PrivateKey]>,
        _verification_keys: impl AsRef<[<T>::PublicKey]>,
        attachment_data: impl AsRef<[u8]>,
    ) -> Result<DecryptedAttachment<T::VerifiedData>, AttachmentError> {
        //TODO: Implement signature verification
        let key_packet_bytes = self.key_packets.decode()?;
        let session_key = pgp_provider
            .new_decryptor()
            .with_decryption_keys(decryption_keys.as_ref())
            .decrypt_session_key(key_packet_bytes)
            .map_err(AttachmentError::SessionKeyDecryption)?;
        pgp_provider
            .new_decryptor()
            .with_session_key(&session_key)
            .decrypt(attachment_data.as_ref(), DataEncoding::Bytes)
            .map_err(AttachmentError::AttachmentDecryption)
            .map(DecryptedAttachment)
    }

    fn decrypt_attachment_from_reader<T: PGPProviderSync, R: io::Read, W: io::Write>(
        &self,
        pgp_provider: &T,
        decryption_keys: impl AsRef<[<T>::PrivateKey]>,
        _verification_keys: impl AsRef<[<T>::PublicKey]>,
        attachment_data: R,
        output_writer: &mut W,
    ) -> Result<AttachmentVerification, AttachmentError> {
        //TODO: Implement signature verification
        let key_packet_bytes = self.key_packets.decode()?;
        let session_key = pgp_provider
            .new_decryptor()
            .with_decryption_keys(decryption_keys.as_ref())
            .decrypt_session_key(key_packet_bytes)
            .map_err(AttachmentError::SessionKeyDecryption)?;
        let mut pt_reader = pgp_provider
            .new_decryptor()
            .with_session_key(&session_key)
            .decrypt_stream(attachment_data, DataEncoding::Bytes)
            .map_err(AttachmentError::AttachmentDecryption)?;
        io::copy(&mut pt_reader, output_writer)
            .map_err(AttachmentError::AttachmentDecryptionWrite)?;
        let status = pt_reader
            .get_verification_status()
            .unwrap_or(VerificationStatus::NotSigned(
                "No signature provided".into(),
            ));
        Ok(AttachmentVerification { status })
    }
}
