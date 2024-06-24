use proton_crypto_account::{
    keys::UnlockedAddressKey,
    proton_crypto::crypto::{
        AsPublicKeyRef, DataEncoding, Encryptor, EncryptorSync, PGPProviderSync,
    },
};

use crate::message::errors::MessageError;

pub trait EncryptableDraft {
    fn plain_text_message_body(&self) -> &[u8];

    fn encrypt_draft_body<T: PGPProviderSync>(
        &self,
        provider: &T,
        address_key: &UnlockedAddressKey<T>,
    ) -> Result<Vec<u8>, MessageError> {
        provider
            .new_encryptor()
            .with_encryption_key(address_key.as_public_key())
            .with_signing_key_refs(&[address_key.as_ref()])
            .with_utf8()
            .encrypt_raw(self.plain_text_message_body(), DataEncoding::Armor)
            .map_err(MessageError::Encryption)
    }
}
