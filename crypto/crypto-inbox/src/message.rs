use std::string::FromUtf8Error;

use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerificationError,
    VerificationResult, VerifiedData, Verifier, VerifierSync,
};

#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("Failed to decrypt the message body: {0}")]
    MessageDecryption(Box<dyn std::error::Error>),
    #[error("Failed to decode message body to utf-8 string: {0}")]
    MessageBodyDecode(#[from] FromUtf8Error),
    #[error("Mime is currently not supported")]
    NotSupportedMime,
}

/// A decrypted e-mail message body that contains signatures for lacy verification.
pub struct DecryptedMessageBody {
    is_decrypted_mime: bool,
    decrypted_body: String,
    // Used for lacy signature verification
    decrypted_raw: Vec<u8>,
    signatures: Vec<u8>,
}

impl AsRef<str> for DecryptedMessageBody {
    fn as_ref(&self) -> &str {
        &self.decrypted_body
    }
}

impl DecryptedMessageBody {
    /// Consumes the type and returns the body of the message.
    pub fn into_string(self) -> String {
        self.decrypted_body
    }

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
            todo!();
        }
        if self.signatures.is_empty() {
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
            .verify_detached(&self.decrypted_raw, &self.signatures, DataEncoding::Bytes)
    }
}

pub trait MessageDecryption {
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
    ) -> Result<DecryptedMessageBody, MessageError> {
        if self.message_is_mime() {
            decrypt_mime(pgp_provider)
        } else {
            decrypt_normal(pgp_provider, decryption_keys, self.message_encrypted_body())
        }
    }
    /// Decrypts the body of the message with a password (EO encrypt once feature).
    fn decrypt_eo<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        passphrase: impl AsRef<str>,
    ) -> Result<DecryptedMessageBody, MessageError> {
        if self.message_is_mime() {
            decrypt_mime(pgp_provider)
        } else {
            let decrypted_message = pgp_provider
                .new_decryptor()
                .with_passphrase(passphrase.as_ref())
                .decrypt(self.message_encrypted_body(), DataEncoding::Armor)
                .map_err(MessageError::MessageDecryption)?;
            // We have to sanitize outside of encryption for lacy signature verification.
            let (decrypted_raw, decrypted_body) =
                to_sanitized_string(decrypted_message.into_vec())?;
            Ok(DecryptedMessageBody {
                is_decrypted_mime: false,
                decrypted_body,
                decrypted_raw,
                signatures: Vec::new(),
            })
        }
    }
}

fn decrypt_mime<T: PGPProviderSync>(
    _pgp_provider: &T,
) -> Result<DecryptedMessageBody, MessageError> {
    Err(MessageError::NotSupportedMime)
}

fn decrypt_normal<T: PGPProviderSync>(
    pgp_provider: &T,
    decryption_keys: &[impl AsRef<T::PrivateKey>],
    data: &[u8],
) -> Result<DecryptedMessageBody, MessageError> {
    let decrypted_message = pgp_provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(data, DataEncoding::Armor)
        .map_err(MessageError::MessageDecryption)?;
    let signatures = decrypted_message.signatures().unwrap_or_default();
    // We have to sanitize outside of encryption for lacy signature verification.
    let (decrypted_raw, decrypted_body) = to_sanitized_string(decrypted_message.into_vec())?;
    Ok(DecryptedMessageBody {
        is_decrypted_mime: false,
        decrypted_body,
        decrypted_raw,
        signatures,
    })
}

fn to_sanitized_string(data: Vec<u8>) -> Result<(Vec<u8>, String), MessageError> {
    let data_as_string = String::from_utf8(data)?;
    let sanitized_body = data_as_string.replace("\r\n", "\n");
    Ok((data_as_string.into_bytes(), sanitized_body))
}
