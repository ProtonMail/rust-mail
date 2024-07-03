use std::io::Write;

use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorSync, DetachedSignatureVariant, Encryptor,
    EncryptorDetachedSignatureWriter, EncryptorSync, PGPProviderSync, Signer, SignerSync,
    VerifiedData, Verifier, VerifierSync,
};

use serde::{Deserialize, Serialize};

use crate::{errors::CardCryptoError, keys::UnlockedUserKey};

crate::string_id! {
    /// An armored signature calculated on a plaintext vcard of a contact
    CardSignature
}

crate::string_id! {
    /// An armored ciphertext calculated on a plaintext vcard of a contact
    EncryptedCard
}

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

pub trait EncryptableAndSignableCard {
    /// Returns a slice of the plaintext card data comprising a contact v-card.
    fn plaintext_card_data(&self) -> &[u8];

    /// Encrypt and and sign the plaintext card data.  This will produce two output values: the encrypted card
    /// and the detached signature calculated over the plaintext card data.
    fn encrypt_and_sign_sync<T: PGPProviderSync>(
        &self,
        provider: &T,
        user_key: &UnlockedUserKey<T>,
    ) -> Result<(EncryptedCard, CardSignature), CardCryptoError> {
        let mut result_data: Vec<u8> = Vec::new();
        let mut encryptor_writer = provider
            .new_encryptor()
            .with_encryption_key(user_key.as_public_key())
            .with_signing_key(user_key.as_ref())
            .with_utf8()
            .encrypt_stream_with_detached_signature(
                &mut result_data,
                DetachedSignatureVariant::Plaintext,
                DataEncoding::Armor,
            )
            .map_err(CardCryptoError::EncryptionError)?;

        encryptor_writer
            .write_all(self.plaintext_card_data())
            .map_err(CardCryptoError::WriteError)?;

        let detached_signature = encryptor_writer
            .finalize_with_detached_signature()
            .map_err(CardCryptoError::EncryptionError)?;

        Ok((
            EncryptedCard(String::from_utf8(result_data).map_err(CardCryptoError::EncodingError)?),
            CardSignature(
                String::from_utf8(detached_signature).map_err(CardCryptoError::EncodingError)?,
            ),
        ))
    }

    /// Sign the plaintext card data.
    fn sign_sync<T: PGPProviderSync>(
        &self,
        provider: &T,
        user_key: &UnlockedUserKey<T>,
    ) -> Result<CardSignature, CardCryptoError> {
        let signature = provider
            .new_signer()
            .with_signing_key(user_key.as_ref())
            .sign_detached(self.plaintext_card_data(), DataEncoding::Armor)
            .map_err(CardCryptoError::SigningError)?;

        Ok(CardSignature(
            String::from_utf8(signature).map_err(CardCryptoError::EncodingError)?,
        ))
    }
}
