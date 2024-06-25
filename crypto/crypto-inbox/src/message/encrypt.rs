use proton_crypto_account::{
    keys::UnlockedAddressKey,
    proton_crypto::crypto::{
        AsPublicKeyRef, DataEncoding, Encryptor, EncryptorSync, PGPProviderSync,
    },
};

use crate::message::errors::MessageError;

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
