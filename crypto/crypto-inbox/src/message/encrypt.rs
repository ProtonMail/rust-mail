use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Encryptor, EncryptorSync, PGPProviderSync,
};

use crate::message::errors::MessageError;

pub trait DraftEncryption {
    fn message_body(&self) -> &[u8];

    fn encrypt_draft<T: PGPProviderSync>(
        &self,
        provider: &T,
        address_key: impl AsRef<T::PrivateKey>,
    ) -> Result<Vec<u8>, MessageError> {
        let address_public_key = provider.private_key_to_public_key(address_key.as_ref())?;
        let encryptor = provider.new_encryptor();
        let binding = [address_key];

        Ok(encryptor
            .with_encryption_key(address_public_key.as_public_key())
            .with_signing_key_refs(&binding)
            .encrypt_raw(self.message_body(), DataEncoding::Armor)?)
    }
}
