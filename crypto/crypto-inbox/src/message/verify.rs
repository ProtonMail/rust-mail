use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, PGPProviderSync, VerificationError, VerificationResult, Verifier,
    VerifierSync,
};
use proton_crypto_account::proton_crypto::utils::to_canonicalized_string;

use proton_crypto_account::proton_crypto::CryptoInfoError;
use proton_crypto_inbox_mime::MimeSignatureVerifier;

pub(crate) fn verify_mime<P>(
    pgp: &P,
    verification_keys: &[impl AsPublicKeyRef<P::PublicKey>],
    data: &[u8],
    signatures: &[u8],
    mime_signatures: &[MimeSignatureVerifier],
) -> VerificationResult
where
    P: PGPProviderSync,
{
    if verification_keys.is_empty() {
        // No verification keys provided.
        return Err(VerificationError::NoVerifier(
            CryptoInfoError::new("No verification keys provided").into(),
        ));
    }
    if !signatures.is_empty() {
        // The encrypted PGP message contained a signature. We prioritize a signature over the whole body.
        return pgp
            .new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(data, signatures, DataEncoding::Bytes);
    }
    let not_signed_error = Err(VerificationError::NotSigned(
        CryptoInfoError::new("No signature found").into(),
    ));
    if mime_signatures.is_empty() {
        // No signature found.
        return not_signed_error;
    }
    // Verify the mime signatures.
    let mut mime_verification_results: Vec<VerificationResult> =
        Vec::with_capacity(mime_signatures.len());
    mime_verification_results.extend(
        mime_signatures
            .iter()
            .map(|verifier| verify_mime_signature(pgp, verification_keys, data, verifier)),
    );
    // Select the ok signature if any else just show the result of the first signature.
    if mime_verification_results.iter().any(Result::is_ok) {
        mime_verification_results
            .into_iter()
            .find(Result::is_ok)
            .unwrap_or(not_signed_error) // Should not happen
    } else {
        mime_verification_results
            .into_iter()
            .next()
            .unwrap_or(not_signed_error) // Should not happen
    }
}

pub(crate) fn verify_normal<P>(
    pgp: &P,
    verification_keys: &[impl AsPublicKeyRef<P::PublicKey>],
    data: &[u8],
    signatures: &[u8],
) -> VerificationResult
where
    P: PGPProviderSync,
{
    if signatures.is_empty() {
        return Err(VerificationError::NotSigned(
            CryptoInfoError::new("No signature found").into(),
        ));
    }

    if verification_keys.is_empty() {
        return Err(VerificationError::NoVerifier(
            CryptoInfoError::new("No verification key provided").into(),
        ));
    }

    pgp.new_verifier()
        .with_verification_key_refs(verification_keys)
        .verify_detached(data, signatures, DataEncoding::Bytes)
}

fn verify_mime_signature<P>(
    pgp: &P,
    verification_keys: &[impl AsPublicKeyRef<P::PublicKey>],
    data: &[u8],
    verifier: &MimeSignatureVerifier,
) -> VerificationResult
where
    P: PGPProviderSync,
{
    let data_to_verify = verifier.data_to_verify(data);

    if let Ok(data_to_verify_sanitized) = to_canonicalized_string(data_to_verify, true) {
        pgp.new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(
                data_to_verify_sanitized,
                verifier.pgp_signature.as_bytes(),
                DataEncoding::Armor,
            )
    } else {
        // Sanitization failed, so we try to verify it without.
        pgp.new_verifier()
            .with_verification_key_refs(verification_keys)
            .verify_detached(
                data_to_verify,
                verifier.pgp_signature.as_bytes(),
                DataEncoding::Armor,
            )
    }
}
