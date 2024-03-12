//! Implements cryptographic helper functions to encrypt and decrypt attachments.
use base64::prelude::*;
use std::{
    fmt::{Display, Formatter},
    io::{self},
};

use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerificationResult,
    VerifiedData, VerifiedDataReader,
};

use crate::string_id;

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
    #[error("Failed to decrypt encrypted detached signature: {0}")]
    EncryptedSignatureDecryption(Box<dyn std::error::Error>),
}

string_id![KeyPackets];

impl KeyPackets {
    pub fn decode(&self) -> Result<Vec<u8>, AttachmentError> {
        let b64 = base64::engine::general_purpose::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::general_purpose::PAD,
        );
        b64.decode(&self.0).map_err(|err| err.into())
    }
}

string_id![AttachmentSignature];

impl AsRef<[u8]> for AttachmentSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

string_id![AttachmentEncryptedSignature];

impl AsRef<[u8]> for AttachmentEncryptedSignature {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

pub struct AttachmentDecrypted<T: VerifiedData>(T);

impl<T: VerifiedData> AsRef<[u8]> for AttachmentDecrypted<T> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl<T: VerifiedData> AttachmentDecrypted<T> {
    pub fn get_verification_status(&self) -> VerificationResult {
        self.0.verification_status()
    }
}

pub struct AttachmentDecryptedReader<'a, R: io::Read + 'a, T: Decryptor<'a>>(
    T::VerifiedDataReader<'a, R>,
);

impl<'a, R: io::Read + 'a, T: Decryptor<'a>> io::Read for AttachmentDecryptedReader<'a, R, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<'a, R: io::Read + 'a, T: Decryptor<'a>> AttachmentDecryptedReader<'a, R, T> {
    pub fn get_verification_status(&self) -> VerificationResult {
        self.0.verification_status()
    }
}

/// Provides a view on the the cryptography relevant fields of the Attachment metadata.
pub trait AttachmentMetadataCryptoView {
    /// Borrows the key packets of the attachment.
    fn get_attachment_key_packets(&self) -> &KeyPackets;
    /// Borrows the signature of the attachment if any.
    fn get_attachment_signature(&self) -> &Option<AttachmentSignature>;
    /// Borrows the encrypted signature of the attachment if any.
    fn get_attachment_encrypted_signature(&self) -> &Option<AttachmentEncryptedSignature>;
}

/// Decrypts an attachment based on its metadata implementing `AttachmentMetadataCryptoView`.
///
/// Decrypts the attachment session key from the key packets with the `decryption_keys`,
/// then uses the session key to decrypt the `attachment_data`, and tries to verify one
/// of the signatures signature/enc_signature if present with the `verification_keys`.
/// The signature verification result can be accessed trough the returned `DecryptedAttachment`.
pub fn decrypt_attachment<T: PGPProviderSync, M: AttachmentMetadataCryptoView>(
    pgp_provider: &T,
    attachment_metadata: &M,
    decryption_keys: &[impl AsRef<T::PrivateKey>],
    verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    attachment_data: impl AsRef<[u8]>,
) -> Result<AttachmentDecrypted<T::VerifiedData>, AttachmentError> {
    let key_packet_bytes = attachment_metadata.get_attachment_key_packets().decode()?;
    let signature_option = attachment_metadata.get_attachment_signature();
    let enc_signature_option = attachment_metadata.get_attachment_encrypted_signature();
    let session_key = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt_session_key(key_packet_bytes)
        .map_err(AttachmentError::SessionKeyDecryption)?;
    let mut decryptor = pgp_provider
        .new_decryptor()
        .with_verification_key_refs(verification_keys)
        .with_session_key_ref(&session_key);
    if let Some(attachment_signature) = signature_option {
        decryptor = decryptor.with_detached_signature_ref(attachment_signature.as_ref(), true)
    } else if let Some(attachment_signature) = enc_signature_option {
        let result = decrypt_and_verify_with_encrypted_signature(
            pgp_provider,
            attachment_signature.as_ref(),
            decryption_keys,
            verification_keys,
            &session_key,
            attachment_data.as_ref(),
        );
        if result.is_ok() {
            // Only consider the signature if no error occurred.
            // On error treat it as no signature provided and fallback.
            return result;
        }
    }
    decryptor
        .decrypt(attachment_data.as_ref(), DataEncoding::Bytes)
        .map_err(AttachmentError::AttachmentDecryption)
        .map(AttachmentDecrypted)
}

/// Decrypts an attachment from an attachment reader based on its metadata implementing `AttachmentMetadataCryptoView`.
///
/// Decrypts the attachment session key from the key packets with the `decryption_keys`,
/// then uses the session key to decrypt the `attachment_data`, and tries to verify one
/// of the signatures signature/enc_signature if present with the `verification_keys`.
/// The signature verification result is returned while the attachment data is written to the `output_writer`.
pub fn decrypt_attachment_from_reader<
    'a,
    T: PGPProviderSync,
    R: io::Read,
    M: AttachmentMetadataCryptoView,
>(
    pgp_provider: &T,
    attachment_metadata: &'a M,
    decryption_keys: &'a [impl AsRef<T::PrivateKey>],
    verification_keys: &'a [impl AsPublicKeyRef<T::PublicKey>],
    attachment_data: R,
) -> Result<AttachmentDecryptedReader<'a, R, T::Decryptor<'a>>, AttachmentError> {
    let key_packet_bytes = attachment_metadata.get_attachment_key_packets().decode()?;
    let signature_option = attachment_metadata.get_attachment_signature();
    let enc_signature_option = attachment_metadata.get_attachment_encrypted_signature();
    let session_key = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt_session_key(key_packet_bytes)
        .map_err(AttachmentError::SessionKeyDecryption)?;
    let mut decryptor = pgp_provider.new_decryptor();
    if let Some(attachment_signature) = signature_option {
        decryptor = decryptor.with_detached_signature_ref(attachment_signature.as_ref(), true)
    } else if let Some(attachment_signature) = enc_signature_option {
        return decrypt_and_verify_with_encrypted_signature_stream(
            pgp_provider,
            attachment_signature.as_ref(),
            decryption_keys,
            verification_keys,
            session_key,
            attachment_data,
        );
    }
    decryptor
        .with_session_key(session_key)
        .with_verification_key_refs(verification_keys)
        .decrypt_stream(attachment_data, DataEncoding::Bytes)
        .map_err(AttachmentError::AttachmentDecryption)
        .map(AttachmentDecryptedReader)
}

fn decrypt_and_verify_with_encrypted_signature<T: PGPProviderSync>(
    pgp_provider: &T,
    enc_signature: &[u8],
    decryption_keys: &[impl AsRef<T::PrivateKey>],
    verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    attachment_session_key: &T::SessionKey,
    attachment_data: &[u8],
) -> Result<AttachmentDecrypted<T::VerifiedData>, AttachmentError> {
    let detached_signature = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(enc_signature, DataEncoding::Armor)
        .map_err(AttachmentError::EncryptedSignatureDecryption)?;
    pgp_provider
        .new_decryptor()
        .with_session_key_ref(attachment_session_key)
        .with_verification_key_refs(verification_keys)
        .with_detached_signature_ref(detached_signature.as_bytes(), false)
        .decrypt(attachment_data, DataEncoding::Bytes)
        .map_err(AttachmentError::AttachmentDecryption)
        .map(AttachmentDecrypted)
}

fn decrypt_and_verify_with_encrypted_signature_stream<'a, T: PGPProviderSync, R: io::Read>(
    pgp_provider: &T,
    enc_signature: &[u8],
    decryption_keys: &'a [impl AsRef<T::PrivateKey>],
    verification_keys: &'a [impl AsPublicKeyRef<T::PublicKey>],
    attachment_session_key: T::SessionKey,
    attachment_data: R,
) -> Result<AttachmentDecryptedReader<'a, R, T::Decryptor<'a>>, AttachmentError> {
    let detached_signature = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(enc_signature, DataEncoding::Armor)
        .map_err(AttachmentError::EncryptedSignatureDecryption)?;
    pgp_provider
        .new_decryptor()
        .with_session_key(attachment_session_key)
        .with_verification_key_refs(verification_keys)
        .with_detached_signature(detached_signature.to_vec(), false)
        .decrypt_stream(attachment_data, DataEncoding::Bytes)
        .map_err(AttachmentError::AttachmentDecryption)
        .map(AttachmentDecryptedReader)
}
