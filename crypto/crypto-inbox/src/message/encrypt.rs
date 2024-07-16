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

pub struct EncryptedMessageBody {
    body: Vec<u8>,
}

impl EncryptedMessageBody {
    pub fn new(body: Vec<u8>) -> EncryptedMessageBody {
        EncryptedMessageBody { body }
    }
    pub fn in_base64(&self) -> String {
        BASE64_STANDARD.encode(&self.body)
    }

    pub fn raw_bytes(&self) -> &[u8] {
        &self.body
    }
}

pub trait EncryptableDraft {
    /// Borrows the plaintext, unencrypted, body of the draft message
    fn plaintext_message_body(&self) -> &[u8];

    /// Encrypts and signs the draft body using the provided address key
    fn encrypt_draft_body<Provider: PGPProviderSync>(
        &self,
        provider: &Provider,
        address_key: &UnlockedAddressKey<Provider>,
    ) -> Result<Vec<u8>, MessageError> {
        provider
            .new_encryptor()
            .with_encryption_key(address_key.as_public_key())
            .with_signing_key(address_key.as_ref())
            .with_utf8()
            .encrypt_raw(self.plaintext_message_body(), DataEncoding::Armor)
            .map_err(MessageError::Encryption)
    }
}

pub trait SessionKeyAndDataPacketsExtractable: GettablePGPMessage {
    /// Extracts the session key and data packets from a PGP message.  The session key returned is decrypted and ready for
    /// use, the data packets returned remain encrypted with the session key.
    ///
    /// The data packets returned are not armored and returned as the raw bytes of the PGP message.
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
            EncryptedMessageBody::new(data_packets),
        ))
    }
}
