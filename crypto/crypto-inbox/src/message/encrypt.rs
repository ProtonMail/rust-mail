use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Encryptor, EncryptorSync, PGPProviderSync,
};

use crate::message::errors::MessageError;

pub trait EncryptableDraft {
    fn plain_text_message_body(&self) -> &[u8];

    fn encrypt_draft_body<T: PGPProviderSync>(
        &self,
        provider: &T,
        address_key: impl AsRef<T::PrivateKey>,
    ) -> Result<Vec<u8>, MessageError> {
        let address_public_key = provider
            .private_key_to_public_key(address_key.as_ref())
            .map_err(MessageError::KeyProblem)?;
        let encryptor = provider.new_encryptor();
        let binding = [address_key];

        encryptor
            .with_encryption_key(address_public_key.as_public_key())
            .with_signing_key_refs(&binding)
            .with_utf8()
            .encrypt_raw(self.plain_text_message_body(), DataEncoding::Armor)
            .map_err(MessageError::Encryption)
    }
}
