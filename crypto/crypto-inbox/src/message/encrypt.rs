use proton_crypto_account::{
    keys::UnlockedAddressKey,
    proton_crypto::crypto::{
        AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, Encryptor, EncryptorSync,
        PGPMessage, PGPProviderSync,
    },
};

use crate::message::errors::MessageError;

use super::GettablePGPMessage;

pub trait EncryptableDraft {
    // Borrows the plaintext, unencrypted, body of the draft message
    fn plaintext_message_body(&self) -> &[u8];

    // Encrypts and signs the draft body using the provided address_key
    fn encrypt_draft_body<T: PGPProviderSync>(
        &self,
        provider: &T,
        address_key: &UnlockedAddressKey<T>,
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
    fn extract_session_key_and_data_packets<T: PGPProviderSync>(
        &self,
        provider: &T,
        decryption_keys: &[impl AsRef<T::PrivateKey>],
    ) -> Result<(T::SessionKey, Vec<u8>), MessageError> {
        let message = provider
            .pgp_message_import(self.pgp_message(), DataEncoding::Armor)
            .map_err(MessageError::ImportProblem)?;

        let key_packets = message.as_key_packets().to_owned();
        let data_packets = message.as_data_packet().to_owned();

        let decrypted_session_key = provider
            .new_decryptor()
            .with_decryption_key_refs(decryption_keys)
            .decrypt_session_key(key_packets)
            .map_err(MessageError::Decryption)?;

        Ok((decrypted_session_key, data_packets))
    }
}
