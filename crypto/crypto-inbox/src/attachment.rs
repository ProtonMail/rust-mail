use base64::prelude::*;
use std::{
    fmt::{Display, Formatter},
    io,
};

use proton_crypto::crypto::{
    DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerificationStatus, VerifiedData,
    VerifiedDataReader,
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
    pub fn get_verification_status(&self) -> AttachmentVerification {
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

pub struct AttachmentDecryptedReader<'a, R: io::Read + 'a, T: Decryptor<'a>>(
    T::VerifiedDataReader<'a, R>,
);

impl<'a, R: io::Read + 'a, T: Decryptor<'a>> io::Read for AttachmentDecryptedReader<'a, R, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<'a, R: io::Read + 'a, T: Decryptor<'a>> AttachmentDecryptedReader<'a, R, T> {
    pub fn get_verification_status(&self) -> AttachmentVerification {
        let status = self
            .0
            .get_verification_status()
            .unwrap_or(VerificationStatus::NotSigned(
                "No signature provided".into(),
            ));
        AttachmentVerification { status }
    }
}

#[derive(Debug, Clone)]
pub struct AttachmentCryptoMetadata {
    pub key_packets: KeyPackets,
    pub signature: Option<AttachmentSignature>,
    pub enc_signature: Option<AttachmentEncryptedSignature>,
}

impl AttachmentCryptoMetadata {
    pub fn new(
        key_packets: KeyPackets,
        signature: Option<AttachmentSignature>,
        enc_signature: Option<AttachmentEncryptedSignature>,
    ) -> Self {
        AttachmentCryptoMetadata {
            key_packets,
            signature,
            enc_signature,
        }
    }
}

impl AttachmentCryptoMetadata {
    /// Decrypts an attachment based on its metadata.
    ///
    /// Decrypts the attachment session key from the key packets with the `decryption_keys`,
    /// then used the session key to decrypt the `attachment_data`, and tries to verify one
    /// of the signatures signature/enc_signature if present with the `verification_keys`.
    /// The signature verification result can be accessed trough the returned `DecryptedAttachment`.
    pub fn decrypt_attachment<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        decryption_keys: impl AsRef<[<T>::PrivateKey]>,
        _verification_keys: impl AsRef<[<T>::PublicKey]>,
        attachment_data: impl AsRef<[u8]>,
    ) -> Result<AttachmentDecrypted<T::VerifiedData>, AttachmentError> {
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
            .map(AttachmentDecrypted)
    }
    /// Decrypts an attachment from an attachment data reader.
    ///
    /// Decrypts the attachment session key from the key packets with the `decryption_keys`,
    /// then used the session key to decrypt the `attachment_data`, and tries to verify one
    /// of the signatures signature/enc_signature if present with the `verification_keys`.
    /// The signature verification result is returned while the attachment data is written to the `output_writer`.
    pub fn decrypt_attachment_from_reader<'a, T: PGPProviderSync, R: io::Read>(
        &self,
        pgp_provider: &T,
        decryption_keys: impl AsRef<[<T>::PrivateKey]>,
        _verification_keys: impl AsRef<[<T>::PublicKey]>,
        attachment_data: R,
    ) -> Result<AttachmentDecryptedReader<'a, R, T::Decryptor<'a>>, AttachmentError> {
        //TODO: Implement signature verification
        let key_packet_bytes = self.key_packets.decode()?;
        let session_key = pgp_provider
            .new_decryptor()
            .with_decryption_keys(decryption_keys.as_ref())
            .decrypt_session_key(key_packet_bytes)
            .map_err(AttachmentError::SessionKeyDecryption)?;
        pgp_provider
            .new_decryptor()
            .with_session_key_move(session_key)
            .decrypt_stream(attachment_data, DataEncoding::Bytes)
            .map_err(AttachmentError::AttachmentDecryption)
            .map(AttachmentDecryptedReader)
    }
}
