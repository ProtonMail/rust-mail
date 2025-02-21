use proton_crypto_account::proton_crypto::{
    crypto::{
        AsPublicKeyRef, DataEncoding, PGPProviderSync, VerificationError, VerificationResult,
        Verifier, VerifierSync,
    },
    CryptoInfoError,
};

/// Allows for lazy notification signature verification
///
pub struct VerifiableNotification {
    decrypted_row: Box<[u8]>,
    signatures: Box<[u8]>,
}

impl VerifiableNotification {
    #[must_use]
    pub(crate) fn new(row: Vec<u8>, signatures: Vec<u8>) -> Self {
        Self {
            decrypted_row: row.into_boxed_slice(),
            signatures: signatures.into_boxed_slice(),
        }
    }

    /// Verifies the message by checking the signature
    ///
    pub fn verify_signature<T: PGPProviderSync>(
        &self,
        pgp_provider: &T,
        verification_keys: &[impl AsPublicKeyRef<T::PublicKey>],
    ) -> VerificationResult {
        if self.signatures.is_empty() {
            return Err(VerificationError::NotSigned(
                CryptoInfoError::new("No signature found").into(),
            ));
        }

        if verification_keys.is_empty() {
            return Err(VerificationError::NoVerifier(
                CryptoInfoError::new("No verification key provided").into(),
            ));
        }

        pgp_provider
            .new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(&self.decrypted_row, &self.signatures, DataEncoding::Bytes)
    }
}
