use crate::message::errors::MessageError;
use base64::{prelude::BASE64_STANDARD, Engine};
use proton_crypto_account::{
    keys::UnlockedAddressKey,
    proton_crypto::crypto::{
        AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, Encryptor, EncryptorSync,
        PGPMessage, PGPProviderSync,
    },
};

use super::GettablePGPMessage;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EncryptedMessageBody(Vec<u8>);

impl EncryptedMessageBody {
    pub fn to_base64_string(&self) -> String {
        BASE64_STANDARD.encode(&self.0)
    }
}

impl AsRef<[u8]> for EncryptedMessageBody {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for EncryptedMessageBody {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

crate::string_id! {
    /// Represents an encrypted an signed draft.
    EncryptedDraft
}

pub trait EncryptableDraft {
    /// Borrows the plaintext, unencrypted, body of the draft message.
    fn plaintext_message_body(&self) -> &[u8];

    /// Encrypts and signs the draft body using the provided `address_key`.
    ///
    /// The output is an armored `OpenPGP` message encoding the encrypted draft.
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The pgp provider instance from [`proton_crypto`].
    /// * `address_key`  - The encryption keys of the recipients to encrypt the attachment to.
    ///
    /// # Errors
    ///
    /// The encryption or encoding fails.
    fn encrypt_draft_body<Provider: PGPProviderSync>(
        &self,
        provider: &Provider,
        address_key: &UnlockedAddressKey<Provider>,
    ) -> Result<EncryptedDraft, MessageError> {
        let encrypted_draft = provider
            .new_encryptor()
            .with_encryption_key(address_key.as_public_key())
            .with_signing_key(address_key.as_ref())
            .with_utf8()
            .encrypt_raw(self.plaintext_message_body(), DataEncoding::Armor)
            .map(String::from_utf8)
            .map_err(MessageError::Encryption)?
            .map(EncryptedDraft)?;
        Ok(encrypted_draft)
    }
}

pub trait SessionKeyAndDataPacketsExtractable: GettablePGPMessage {
    /// Extracts the session key and data packets from a PGP message.  The session key returned is decrypted and ready for
    /// use, the data packets returned remain encrypted with the session key.
    ///
    /// The data packets returned are not armored and returned as the raw bytes of the PGP message.
    ///
    /// # Parameters
    ///
    /// * `provider` - The PGP implementation providing the functions required for message importing, separation, and decryption.
    /// * `decryption_keys` - The set of PGP private keys to be used when attempting to decrypt the session key packet.
    ///
    /// # Errors
    ///
    /// Returns a `MessageError` if the PGP message may not be imported (if it is malformed), or if decrypting the session key packet
    /// fails.
    fn extract_session_key_and_data_packets<Provider: PGPProviderSync>(
        &self,
        provider: &Provider,
        decryption_keys: &[impl AsRef<Provider::PrivateKey>],
    ) -> Result<(Provider::SessionKey, EncryptedMessageBody), MessageError> {
        let message = provider
            .pgp_message_import(self.pgp_message(), DataEncoding::Armor)
            .map_err(MessageError::ImportProblem)?;

        let key_packets = message.as_key_packets();
        let data_packets = message.as_data_packet().to_owned();

        let decrypted_session_key = provider
            .new_decryptor()
            .with_decryption_key_refs(decryption_keys)
            .decrypt_session_key(key_packets)
            .map_err(MessageError::Decryption)?;

        Ok((
            decrypted_session_key,
            EncryptedMessageBody::from(data_packets),
        ))
    }
}
