use proton_crypto_account::proton_crypto::CryptoInfoError;
use proton_crypto_account::proton_crypto::crypto::VerificationError;
use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerificationResult,
    VerifiedData,
};

use proton_crypto_inbox_mime::{MimeProcessor, ProcessMime, ProcessedMessage};

use crate::message;

use super::GettablePGPMessage;
use super::MessageError;
use super::utils::to_sanitized_string;

/// A raw decrypted message body.
///
/// A decrypted message body either contains a raw mime message or a plain message.
/// It is possible to verify the signatures on the raw body after decryption with
/// [`RawDecryptedBody::verify_signature`] method.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum RawDecryptedBody {
    Plain {
        raw_body: Vec<u8>,
        signatures: Vec<u8>,
    },
    Mime {
        message_id: String,
        raw_body: Vec<u8>,
        signatures: Vec<u8>,
    },
}

impl RawDecryptedBody {
    #[must_use]
    pub fn new_plain(raw_body: Vec<u8>, signatures: Vec<u8>) -> Self {
        Self::Plain {
            raw_body,
            signatures,
        }
    }

    #[must_use]
    pub fn new_mime(message_id: &str, raw_body: Vec<u8>, signatures: Vec<u8>) -> Self {
        Self::Mime {
            message_id: message_id.to_string(),
            raw_body,
            signatures,
        }
    }

    /// Converts the the decrypted body to readable string data.
    ///
    /// If the body is a mime message, the body is processed to a [`ProcessedMessage`] struct.
    pub fn processed_body(&self) -> Result<DecryptedBody, MessageError> {
        match self {
            RawDecryptedBody::Plain {
                raw_body,
                signatures: _,
            } => {
                let decrypted_body = to_sanitized_string(raw_body)?;
                Ok(DecryptedBody::Plain(decrypted_body))
            }
            RawDecryptedBody::Mime {
                message_id,
                raw_body: raw_message,
                signatures: _,
            } => {
                let processed_message = MimeProcessor::process_mime(message_id, raw_message)?;
                Ok(DecryptedBody::Mime(processed_message))
            }
        }
    }

    /// Allows to verify the signatures of the message after decryption.
    ///
    /// The signatures verification is separate because the fetch/verification
    /// of the public keys might take longer.
    /// Thus, the UI might show the decrypted body before the verification result is shown (e.g., with locks).
    pub fn verify_signature<P>(
        &self,
        pgp: &P,
        verification_keys: &[impl AsPublicKeyRef<P::PublicKey>],
    ) -> VerificationResult
    where
        P: PGPProviderSync,
    {
        match self {
            RawDecryptedBody::Plain {
                raw_body,
                signatures,
            } => message::verify_normal(pgp, verification_keys, raw_body, signatures),
            RawDecryptedBody::Mime {
                message_id,
                raw_body: raw_message,
                signatures,
            } => {
                let internal_signatures = MimeProcessor::process_mime(message_id, raw_message)
                    .map(|message| message.signatures)
                    .map_err(|_| {
                        VerificationError::RuntimeError(
                            CryptoInfoError::new("Failed to extract signatures from mime message")
                                .into(),
                        )
                    })?;
                message::verify_mime(
                    pgp,
                    verification_keys,
                    raw_message,
                    signatures,
                    &internal_signatures,
                )
            }
        }
    }
}

impl TryFrom<RawDecryptedBody> for DecryptedBody {
    type Error = MessageError;

    fn try_from(value: RawDecryptedBody) -> Result<Self, Self::Error> {
        value.processed_body()
    }
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
    #[must_use]
    pub fn body(&self) -> &str {
        self.as_ref()
    }

    /// Consumes the type and returns the body of the message.
    #[must_use]
    pub fn into_string(self) -> String {
        match self {
            DecryptedBody::Plain(body) => body,
            DecryptedBody::Mime(mime_body) => mime_body.body,
        }
    }

    /// Returns whether this decryption result is from an encrypted mime message.
    #[must_use]
    pub fn is_mime(&self) -> bool {
        matches!(self, DecryptedBody::Mime(_))
    }
}

#[allow(clippy::module_name_repetitions)]
pub trait DecryptableMessage: GettablePGPMessage {
    /// Borrows the unique id of the message.
    fn message_id(&self) -> Option<&str>;

    /// Indicates wether the message is mime.
    ///
    /// If it returns true mime decryption is triggered.
    ///
    /// Must return true if the `MIMEType` of the message is `multipart/mixed`.
    fn message_is_mime(&self) -> bool;

    /// Decrypts the body of the message.
    ///
    /// This method does not perform signature verification, but returns a
    /// `DecryptedMessageBody` result on which the signatures can be verified
    /// at a later point.
    fn decrypt<P>(
        &self,
        pgp: &P,
        decryption_keys: &[impl AsRef<P::PrivateKey>],
    ) -> Result<RawDecryptedBody, MessageError>
    where
        P: PGPProviderSync,
    {
        if self.message_is_mime() {
            decrypt_mime(
                pgp,
                decryption_keys,
                self.pgp_message(),
                self.message_id().ok_or(MessageError::MissingMessageID)?,
            )
        } else {
            decrypt_normal(pgp, decryption_keys, self.pgp_message())
        }
    }

    /// Decrypts the body of the message with a password (EO encrypt-once feature).
    fn decrypt_encrypt_once<P>(
        &self,
        pgp: &P,
        passphrase: impl AsRef<str>,
    ) -> Result<RawDecryptedBody, MessageError>
    where
        P: PGPProviderSync,
    {
        let decrypted_message = pgp
            .new_decryptor()
            .with_passphrase(passphrase.as_ref())
            .with_ut8_sanitization()
            .decrypt(self.pgp_message(), DataEncoding::Armor)
            .map_err(MessageError::Decryption)?;

        Ok(RawDecryptedBody::Plain {
            raw_body: decrypted_message.into_vec(),
            signatures: Vec::new(),
        })
    }
}

fn decrypt_mime<P>(
    pgp: &P,
    decryption_keys: &[impl AsRef<P::PrivateKey>],
    data: &[u8],
    message_id: &str,
) -> Result<RawDecryptedBody, MessageError>
where
    P: PGPProviderSync,
{
    let decrypted_body = pgp
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(data, DataEncoding::Armor)
        .map_err(MessageError::Decryption)?;

    let signatures = decrypted_body.signatures().unwrap_or_default();
    let raw_mime_data = decrypted_body.into_vec();

    Ok(RawDecryptedBody::new_mime(
        message_id,
        raw_mime_data,
        signatures,
    ))
}

fn decrypt_normal<P>(
    pgp: &P,
    decryption_keys: &[impl AsRef<P::PrivateKey>],
    data: &[u8],
) -> Result<RawDecryptedBody, MessageError>
where
    P: PGPProviderSync,
{
    let decrypted_message = pgp
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(data, DataEncoding::Armor)
        .map_err(MessageError::Decryption)?;

    let signatures = decrypted_message.signatures().unwrap_or_default();

    Ok(RawDecryptedBody::new_plain(
        decrypted_message.into_vec(),
        signatures,
    ))
}
