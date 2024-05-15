use std::io;

use proton_crypto_account::proton_crypto::{
    crypto::{
        AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, DetachedSignatureVariant,
        PGPProviderSync, VerificationResult, VerifiedData, VerifiedDataReader,
    },
    CryptoError,
};

use super::{AttachmentEncryptedSignature, AttachmentSignature, KeyPackets};

/// Errors thrown by attachment decryption.
#[derive(Debug, thiserror::Error)]
pub enum AttachmentDecryptionError {
    #[error("Could not decode key packets: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Failed to decrypt key packets to session key with the decryption keys: {0}")]
    SessionKeyDecryption(CryptoError),
    #[error("Failed to decrypt attachment with the extracted session key: {0}")]
    AttachmentDecryption(CryptoError),
    #[error("Failed to decrypt and write to the output writer: {0}")]
    AttachmentDecryptionWrite(io::Error),
    #[error("Failed to decrypt encrypted detached signature: {0}")]
    EncryptedSignatureDecryption(CryptoError),
}

/// Represents decryption result of a decrypted attachment.
pub struct DecryptedAttachment<T: VerifiedData>(T);

impl<T: VerifiedData> AsRef<[u8]> for DecryptedAttachment<T> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<T: VerifiedData> DecryptedAttachment<T> {
    /// Returns the signature verification result of the data that has been read.
    pub fn verification_result(&self) -> VerificationResult {
        self.0.verification_result()
    }
    /// Returns a byte slice of the attachments content.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
    /// Returns a vector of the attachments content.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

/// Reader for reading decrypted attachment data.
pub struct DecryptedAttachmentReader<'a, R: io::Read + 'a, T: Decryptor<'a>>(
    T::VerifiedDataReader<'a, R>,
);

impl<'a, R: io::Read + 'a, T: Decryptor<'a>> io::Read for DecryptedAttachmentReader<'a, R, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<'a, R: io::Read + 'a, T: Decryptor<'a>> DecryptedAttachmentReader<'a, R, T> {
    /// Returns the signature verification result of the data that has been read.
    pub fn verification_result(self) -> VerificationResult {
        self.0.verification_result()
    }
}

/// Provides default implementation for attachment decryption
/// and only requires to implement the view methods on the attachment metadata.
pub trait AttachmentDecryption {
    /// Borrows the key packets of the attachment.
    fn attachment_key_packets(&self) -> &KeyPackets;
    /// Borrows the signature of the attachment if any.
    fn attachment_signature(&self) -> &Option<AttachmentSignature>;
    /// Borrows the encrypted signature of the attachment if any.
    fn attachment_encrypted_signature(&self) -> &Option<AttachmentEncryptedSignature>;
    /// Decrypts an attachment based on its metadata.
    ///
    /// Decrypts the attachment session key from the key packets with the `decryption_keys`,
    /// then uses the session key to decrypt the `attachment_data`, and tries to verify one
    /// of the signatures `signature/enc_signature` if present with the `verification_keys`.
    /// The signature verification result can be accessed trough the returned `DecryptedAttachment`.
    fn decrypt<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        decryption_keys: &[impl AsRef<T::PrivateKey>],
        verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
        attachment_data: impl AsRef<[u8]>,
    ) -> Result<DecryptedAttachment<T::VerifiedData>, AttachmentDecryptionError> {
        let key_packet_bytes = self.attachment_key_packets().decode()?;
        let signature_option = self.attachment_signature();
        let enc_signature_option = self.attachment_encrypted_signature();
        let session_key = pgp_provider
            .new_decryptor()
            .with_decryption_key_refs(decryption_keys)
            .decrypt_session_key(key_packet_bytes)
            .map_err(AttachmentDecryptionError::SessionKeyDecryption)?;
        let mut decryptor = pgp_provider
            .new_decryptor()
            .with_verification_key_refs(verification_keys)
            .with_session_key_ref(&session_key);
        if let Some(attachment_signature) = signature_option {
            decryptor = decryptor.with_detached_signature_ref(
                attachment_signature.as_ref(),
                DetachedSignatureVariant::Plaintext,
                true,
            );
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
            .map_err(AttachmentDecryptionError::AttachmentDecryption)
            .map(DecryptedAttachment)
    }
    /// Decrypts an attachment from an attachment reader based on its metadata.
    ///
    /// Decrypts the attachment session key from the key packets with the `decryption_keys`,
    /// then uses the session key to decrypt the `attachment_data`, and tries to verify one
    /// of the signatures `signature/enc_signature` if present with the `verification_keys`.
    /// The signature verification result is returned while the attachment data is written to the `output_writer`.
    fn decrypt_from_reader<'a, T: PGPProviderSync, R: io::Read>(
        &'a self,
        pgp_provider: &T,
        decryption_keys: &'a [impl AsRef<T::PrivateKey>],
        verification_keys: &'a [impl AsPublicKeyRef<T::PublicKey>],
        attachment_data: R,
    ) -> Result<DecryptedAttachmentReader<'a, R, T::Decryptor<'a>>, AttachmentDecryptionError> {
        let key_packet_bytes = self.attachment_key_packets().decode()?;
        let signature_option = self.attachment_signature();
        let enc_signature_option = self.attachment_encrypted_signature();
        let session_key = pgp_provider
            .new_decryptor()
            .with_decryption_key_refs(decryption_keys)
            .decrypt_session_key(key_packet_bytes)
            .map_err(AttachmentDecryptionError::SessionKeyDecryption)?;
        let mut decryptor = pgp_provider.new_decryptor();
        if let Some(attachment_signature) = signature_option {
            decryptor = decryptor.with_detached_signature_ref(
                attachment_signature.as_ref(),
                DetachedSignatureVariant::Plaintext,
                true,
            );
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
            .map_err(AttachmentDecryptionError::AttachmentDecryption)
            .map(DecryptedAttachmentReader)
    }
}

fn decrypt_and_verify_with_encrypted_signature<T: PGPProviderSync>(
    pgp_provider: &T,
    enc_signature: &[u8],
    decryption_keys: &[impl AsRef<T::PrivateKey>],
    verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    attachment_session_key: &T::SessionKey,
    attachment_data: &[u8],
) -> Result<DecryptedAttachment<T::VerifiedData>, AttachmentDecryptionError> {
    let detached_signature = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(enc_signature, DataEncoding::Armor)
        .map_err(AttachmentDecryptionError::EncryptedSignatureDecryption)?;
    pgp_provider
        .new_decryptor()
        .with_session_key_ref(attachment_session_key)
        .with_verification_key_refs(verification_keys)
        .with_detached_signature_ref(
            detached_signature.as_bytes(),
            DetachedSignatureVariant::Plaintext,
            false,
        )
        .decrypt(attachment_data, DataEncoding::Bytes)
        .map_err(AttachmentDecryptionError::AttachmentDecryption)
        .map(DecryptedAttachment)
}

fn decrypt_and_verify_with_encrypted_signature_stream<'a, T: PGPProviderSync, R: io::Read>(
    pgp_provider: &T,
    enc_signature: &[u8],
    decryption_keys: &'a [impl AsRef<T::PrivateKey>],
    verification_keys: &'a [impl AsPublicKeyRef<T::PublicKey>],
    attachment_session_key: T::SessionKey,
    attachment_data: R,
) -> Result<DecryptedAttachmentReader<'a, R, T::Decryptor<'a>>, AttachmentDecryptionError> {
    let detached_signature = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(enc_signature, DataEncoding::Armor)
        .map_err(AttachmentDecryptionError::EncryptedSignatureDecryption)?;
    pgp_provider
        .new_decryptor()
        .with_session_key(attachment_session_key)
        .with_verification_key_refs(verification_keys)
        .with_detached_signature(
            detached_signature.to_vec(),
            DetachedSignatureVariant::Plaintext,
            false,
        )
        .decrypt_stream(attachment_data, DataEncoding::Bytes)
        .map_err(AttachmentDecryptionError::AttachmentDecryption)
        .map(DecryptedAttachmentReader)
}
