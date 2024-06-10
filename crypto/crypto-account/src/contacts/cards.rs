use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerifiedData,
    Verifier, VerifierSync,
};

use crate::errors::CardCryptoError;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum ContactCardType {
    ClearText = 0,
    Encrypted = 1,
    Signed = 2,
    EncryptedAndSigned = 3,
}

pub trait CardCryptography {
    fn card_type(&self) -> ContactCardType;

    fn card_data(&self) -> &[u8];

    fn card_signature(&self) -> &[u8];

    fn decrypt_and_verify_sync<T: PGPProviderSync>(
        &self,
        provider: &T,
        decryption_keys: &[impl AsRef<T::PrivateKey>],
        verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    ) -> Result<Vec<u8>, CardCryptoError> {
        match self.card_type() {
            ContactCardType::ClearText => Ok(self.card_data().to_owned()),
            ContactCardType::Encrypted => {
                if decryption_keys.is_empty() {
                    return Err(CardCryptoError::MissingDecryptionKey());
                }

                Ok(provider
                    .new_decryptor()
                    .with_decryption_key_refs(decryption_keys)
                    .decrypt(self.card_data(), DataEncoding::Armor)
                    .map_err(CardCryptoError::DecryptionError)?
                    .into_vec())
            }
            ContactCardType::Signed => {
                if verification_keys.is_empty() {
                    return Err(CardCryptoError::MissingVerificationKey());
                }

                provider
                    .new_verifier()
                    .with_verification_key_refs(verification_keys)
                    .verify_detached(self.card_data(), self.card_signature(), DataEncoding::Armor)
                    .map_err(CardCryptoError::SignatureVerificationError)?;

                return Ok(self.card_data().to_owned());
            }
            ContactCardType::EncryptedAndSigned => {
                if decryption_keys.is_empty() {
                    return Err(CardCryptoError::MissingDecryptionKey());
                }
                if verification_keys.is_empty() {
                    return Err(CardCryptoError::MissingVerificationKey());
                }

                let decrypted_card = provider
                    .new_decryptor()
                    .with_decryption_key_refs(decryption_keys)
                    .decrypt(self.card_data(), DataEncoding::Armor)
                    .map_err(CardCryptoError::DecryptionError)?
                    .into_vec();

                provider
                    .new_verifier()
                    .with_verification_key_refs(verification_keys)
                    .verify_detached(&decrypted_card, self.card_signature(), DataEncoding::Armor)
                    .map_err(CardCryptoError::SignatureVerificationError)?;

                Ok(decrypted_card)
            }
        }
    }
}
