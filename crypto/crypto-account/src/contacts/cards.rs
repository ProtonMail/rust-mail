use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, DetachedSignatureVariant,
    PGPProviderSync, VerifiedData, Verifier, VerifierSync,
};

use crate::errors::CardCryptoError;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum ContactCardType {
    ClearText = 0,
    Encrypted = 1,
    Signed = 2,
    EncryptedAndSigned = 3,
}

pub trait DecryptableVerifiableCard {
    /// Returns the card's crypto type.
    fn card_type(&self) -> ContactCardType;

    /// Returns the raw card data, which is either encrypted or in plain text.
    fn card_data(&self) -> &[u8];

    /// Returns the raw detached signature of the card if any.
    fn card_signature(&self) -> Option<&[u8]>;

    /// Returns the plain text data from the card.  If the card has been encrypted, it is decrypted.  If the card
    /// is signed, the signature is verified.
    ///
    /// # Errors
    /// When decryption or signature verification fail
    fn decrypt_and_verify_sync<T: PGPProviderSync>(
        &self,
        provider: &T,
        decryption_keys: &[impl AsRef<T::PrivateKey>],
        verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    ) -> Result<Vec<u8>, CardCryptoError> {
        match self.card_type() {
            ContactCardType::ClearText => Ok(self.card_data().to_owned()),
            ContactCardType::Encrypted => Ok(provider
                .new_decryptor()
                .with_decryption_key_refs(decryption_keys)
                .decrypt(self.card_data(), DataEncoding::Armor)
                .map_err(CardCryptoError::DecryptionError)?
                .into_vec()),
            ContactCardType::Signed => {
                provider
                    .new_verifier()
                    .with_verification_key_refs(verification_keys)
                    .verify_detached(
                        self.card_data(),
                        self.card_signature().ok_or(CardCryptoError::NoSignature)?,
                        DataEncoding::Armor,
                    )
                    .map_err(CardCryptoError::SignatureVerificationError)?;

                return Ok(self.card_data().to_owned());
            }
            ContactCardType::EncryptedAndSigned => {
                let decrypted_card_result = provider
                    .new_decryptor()
                    .with_decryption_key_refs(decryption_keys)
                    .with_verification_key_refs(verification_keys)
                    .with_detached_signature_ref(
                        self.card_signature().ok_or(CardCryptoError::NoSignature)?,
                        DetachedSignatureVariant::Plaintext,
                        true,
                    )
                    .decrypt(self.card_data(), DataEncoding::Armor)
                    .map_err(CardCryptoError::DecryptionError)?;
                decrypted_card_result.verification_result()?;
                Ok(decrypted_card_result.into_vec())
            }
        }
    }
}
