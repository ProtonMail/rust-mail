use proton_crypto_account::proton_crypto::crypto::{
    DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerifiedData,
};

use proton_crypto_inbox_mime::{MimeProcessor, ProcessMime, ProcessedMessage};

use super::GettablePGPMessage;
use super::MessageError;
use super::VerifiableBody;
use super::utils::to_sanitized_string;

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
    ) -> Result<(DecryptedBody, VerifiableBody), MessageError>
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
    ) -> Result<DecryptedBody, MessageError>
    where
        P: PGPProviderSync,
    {
        let decrypted_message = pgp
            .new_decryptor()
            .with_passphrase(passphrase.as_ref())
            .with_ut8_sanitization()
            .decrypt(self.pgp_message(), DataEncoding::Armor)
            .map_err(MessageError::Decryption)?;

        let decoded_message = std::str::from_utf8(decrypted_message.as_bytes())?;

        Ok(DecryptedBody::Plain(decoded_message.to_owned()))
    }
}

fn decrypt_mime<P>(
    pgp: &P,
    decryption_keys: &[impl AsRef<P::PrivateKey>],
    data: &[u8],
    message_id: &str,
) -> Result<(DecryptedBody, VerifiableBody), MessageError>
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

    let (mime_body_data, mime_signatures) =
        MimeProcessor::process_mime(message_id, &raw_mime_data)?;

    let decrypted_mime_body = DecryptedBody::Mime(mime_body_data);
    let verifier = VerifiableBody::new(true, raw_mime_data, signatures, mime_signatures);

    Ok((decrypted_mime_body, verifier))
}

fn decrypt_normal<P>(
    pgp: &P,
    decryption_keys: &[impl AsRef<P::PrivateKey>],
    data: &[u8],
) -> Result<(DecryptedBody, VerifiableBody), MessageError>
where
    P: PGPProviderSync,
{
    let decrypted_message = pgp
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .decrypt(data, DataEncoding::Armor)
        .map_err(MessageError::Decryption)?;

    let signatures = decrypted_message.signatures().unwrap_or_default();

    // We have to sanitize outside of encryption for lazy signature verification.
    let decrypted_body = to_sanitized_string(decrypted_message.as_bytes())?;
    let decrypted_body = DecryptedBody::Plain(decrypted_body);
    let verifier = VerifiableBody::new(false, decrypted_message.into_vec(), signatures, Vec::new());

    Ok((decrypted_body, verifier))
}
