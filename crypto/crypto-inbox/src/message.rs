use std::string::FromUtf8Error;

use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerificationError,
    VerificationResult, VerifiedData, Verifier, VerifierSync,
};
use proton_crypto_inbox_mime::{
    MimeProcessor, MimeSignatureVerifier, ProcessMime, ProcessMimeError, ProcessedMessage,
};

#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("Failed to decrypt the message body: {0}")]
    Decryption(Box<dyn std::error::Error>),
    #[error("Failed to decode message body to utf-8 string: {0}")]
    BodyDecode(#[from] FromUtf8Error),
    #[error("Failed to decode mime message body: {0}")]
    MimeBodyDecode(#[from] ProcessMimeError),
    #[error("Mime is currently not supported")]
    NotSupportedMime,
}

/// A decrypted message body that either contains a plain body or a decrypted `mime` body.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum DecryptedBody {
    Plain(String),
    Mime(ProcessedMessage),
}

impl AsRef<str> for DecryptedBody {
    fn as_ref(&self) -> &str {
        match self {
            DecryptedBody::Plain(body) => body,
            DecryptedBody::Mime(mime_body) => &mime_body.body,
        }
    }
}

impl DecryptedBody {
    /// Returns a reference to the decrypted message body.
    pub fn body(&self) -> &str {
        self.as_ref()
    }
    /// Consumes the type and returns the body of the message.
    pub fn into_string(self) -> String {
        match self {
            DecryptedBody::Plain(body) => body,
            DecryptedBody::Mime(mime_body) => mime_body.body,
        }
    }
    /// Returns whether this decryption result is from an encrypted mime message.
    pub fn is_mime(&self) -> bool {
        matches!(self, DecryptedBody::Mime(_))
    }
}

/// Allows for lazy message body signature verification.
#[derive(Debug, Clone)]
pub struct VerifiableBody {
    is_decrypted_mime: bool,
    decrypted_raw: Vec<u8>,
    signatures: Vec<u8>,
    mime_signatures: Vec<MimeSignatureVerifier>,
}

impl VerifiableBody {
    /// Allows to verify the signatures of the message after decryption.
    ///
    /// The signatures verification is separate because the fetch/verification
    /// of the public keys might take longer.
    /// Thus, the UI might show the decrypted body before the verification result is shown (e.g., with locks).
    pub fn verify_signature<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    ) -> VerificationResult {
        if self.is_decrypted_mime {
            verify_mime(
                pgp_provider,
                verification_keys,
                &self.decrypted_raw,
                &self.signatures,
                &self.mime_signatures,
            )
        } else {
            verify_normal(
                pgp_provider,
                verification_keys,
                &self.decrypted_raw,
                &self.signatures,
            )
        }
    }
}

pub trait DecryptableMessage {
    /// Borrows the unique id of the message.
    fn message_id(&self) -> &str;
    /// Indicates wether the message is mime.
    ///
    /// If it returns true mime decryption is triggered.
    fn message_is_mime(&self) -> bool;
    /// Returns a reference to the encrypted body of the message.
    fn message_encrypted_body(&self) -> &[u8];
    /// Decrypts the body of the message.
    ///
    /// This method does not perform signature verification, but returns a
    /// `DecryptedMessageBody` result on which the signatures can be verified
    /// at a later point.
    fn decrypt<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        decryption_keys: &[impl AsRef<T::PrivateKey>],
    ) -> Result<(DecryptedBody, VerifiableBody), MessageError> {
        if self.message_is_mime() {
            decrypt_mime(
                pgp_provider,
                decryption_keys,
                self.message_encrypted_body(),
                self.message_id(),
            )
        } else {
            decrypt_normal(pgp_provider, decryption_keys, self.message_encrypted_body())
        }
    }
    /// Decrypts the body of the message with a password (EO encrypt-once feature).
    fn decrypt_encrypt_once<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        passphrase: impl AsRef<str>,
    ) -> Result<DecryptedBody, MessageError> {
        let decrypted_message = pgp_provider
            .new_decryptor()
            .with_passphrase(passphrase.as_ref())
            .with_ut8_sanitization()
            .decrypt(self.message_encrypted_body(), DataEncoding::Armor)
            .map_err(MessageError::Decryption)?;
        let decoded_message = String::from_utf8(decrypted_message.into_vec())?;
        Ok(DecryptedBody::Plain(decoded_message))
    }
}

fn decrypt_mime<T: PGPProviderSync>(
    pgp_provider: &T,
    decryption_keys: &[impl AsRef<T::PrivateKey>],
    data: &[u8],
    message_id: &str,
) -> Result<(DecryptedBody, VerifiableBody), MessageError> {
    let decrypted_body = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(data, DataEncoding::Armor)
        .map_err(MessageError::Decryption)?;
    let signatures = decrypted_body.signatures().unwrap_or_default();
    let raw_mime_data = decrypted_body.into_vec();
    let (mime_body_data, mime_signatures) =
        MimeProcessor::process_mime(message_id, &raw_mime_data)?;
    let decrypted_mime_body = DecryptedBody::Mime(mime_body_data);
    let verifier = VerifiableBody {
        is_decrypted_mime: true,
        decrypted_raw: raw_mime_data,
        signatures,
        mime_signatures,
    };
    Ok((decrypted_mime_body, verifier))
}

fn decrypt_normal<T: PGPProviderSync>(
    pgp_provider: &T,
    decryption_keys: &[impl AsRef<T::PrivateKey>],
    data: &[u8],
) -> Result<(DecryptedBody, VerifiableBody), MessageError> {
    let decrypted_message = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(data, DataEncoding::Armor)
        .map_err(MessageError::Decryption)?;
    let signatures = decrypted_message.signatures().unwrap_or_default();
    // We have to sanitize outside of encryption for lazy signature verification.
    let (decrypted_raw, decrypted_body) = to_sanitized_string(decrypted_message.into_vec())?;
    let decrypted_body = DecryptedBody::Plain(decrypted_body);
    let verifier = VerifiableBody {
        is_decrypted_mime: false,
        decrypted_raw,
        signatures,
        mime_signatures: Vec::new(),
    };
    Ok((decrypted_body, verifier))
}

fn verify_mime<T: PGPProviderSync>(
    pgp_provider: &T,
    verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    data: &[u8],
    signatures: &[u8],
    mime_signatures: &[MimeSignatureVerifier],
) -> VerificationResult {
    if verification_keys.is_empty() {
        // No verification keys provided.
        return Err(VerificationError::NoVerifier(
            "No verification keys provided".into(),
        ));
    }
    if !signatures.is_empty() {
        // The encrypted PGP message contained a signature. We prioritize a signature over the whole body.
        return pgp_provider
            .new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(data, signatures, DataEncoding::Bytes);
    }
    let not_signed_error = Err(VerificationError::NotSigned("No signature found".into()));
    if mime_signatures.is_empty() {
        // No signature found.
        return not_signed_error;
    }
    // Verify the mime signatures.
    let mut mime_verification_results: Vec<VerificationResult> =
        Vec::with_capacity(mime_signatures.len());
    mime_verification_results.extend(
        mime_signatures
            .iter()
            .map(|verifier| verify_mime_signature(pgp_provider, verification_keys, data, verifier)),
    );
    // Select the ok signature if any else just show the result of the first signature.
    if mime_verification_results
        .iter()
        .any(|result| result.is_ok())
    {
        mime_verification_results
            .into_iter()
            .find(|result| result.is_ok())
            .unwrap_or(not_signed_error) // Should not happen
    } else {
        mime_verification_results
            .into_iter()
            .next()
            .unwrap_or(not_signed_error) // Should not happen
    }
}

fn verify_normal<T: PGPProviderSync>(
    pgp_provider: &T,
    verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    data: &[u8],
    signatures: &[u8],
) -> VerificationResult {
    if signatures.is_empty() {
        return Err(VerificationError::NotSigned("No signature found".into()));
    }
    if verification_keys.is_empty() {
        return Err(VerificationError::NoVerifier(
            "No verification key provided".into(),
        ));
    }
    pgp_provider
        .new_verifier()
        .with_verification_key_refs(verification_keys)
        .verify_detached(data, signatures, DataEncoding::Bytes)
}

fn verify_mime_signature<T: PGPProviderSync>(
    pgp_provider: &T,
    verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    data: &[u8],
    verifier: &MimeSignatureVerifier,
) -> VerificationResult {
    let data_to_verify = verifier.data_to_verify(data);
    if let Ok(data_to_verify_sanitized) = to_canonicalized_verify_string(data_to_verify) {
        pgp_provider
            .new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(
                data_to_verify_sanitized,
                verifier.pgp_signature.as_bytes(),
                DataEncoding::Armor,
            )
    } else {
        // Sanitization failed, so we try to verify it without.
        pgp_provider
            .new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(
                data_to_verify,
                verifier.pgp_signature.as_bytes(),
                DataEncoding::Armor,
            )
    }
}

fn to_sanitized_string(data: Vec<u8>) -> Result<(Vec<u8>, String), MessageError> {
    let data_as_string = String::from_utf8(data)?;
    let sanitized_body = data_as_string.replace("\r\n", "\n");
    Ok((data_as_string.into_bytes(), sanitized_body))
}

fn to_canonicalized_verify_string(data: &[u8]) -> Result<String, MessageError> {
    let data = String::from_utf8(data.to_vec())?
        .replace("\r\n", "\n")
        .replace('\n', "\r\n"); //TODO Improve inefficient transformation.
    Ok(data)
}
