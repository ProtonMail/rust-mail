use proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Decryptor, DecryptorAsync, DecryptorSync, PGPProviderAsync,
    PGPProviderSync, VerifiedData,
};

use crate::errors::AccountCryptoError;

/// Decrypts a token associated with key to unlock it.
///
/// Decrypts and verifies the token with the provided keys.
/// If signature verification fails, it returns an error.
pub fn decrypt_key_token<Prov: PGPProviderSync>(
    provider: &Prov,
    token: &str,
    signature: &str,
    decryption_keys: &[impl AsRef<Prov::PrivateKey>],
    verification_keys: &[impl AsPublicKeyRef<Prov::PublicKey>],
    verification_context: Option<Prov::VerificationContext>,
) -> Result<Vec<u8>, AccountCryptoError> {
    let mut decryptor = provider
        .new_decryptor()
        .with_decryption_key_refs(decryption_keys)
        .with_verification_key_refs(verification_keys)
        .with_detached_signature_ref(signature.as_bytes(), true);
    if let Some(context) = &verification_context {
        decryptor = decryptor.with_verification_context(context)
    }
    let verified_data = decryptor
        .decrypt(token.as_bytes(), DataEncoding::Armor)
        .map_err(AccountCryptoError::TokenDecryption)?;
    verified_data
        .verification_result()
        .map_err(AccountCryptoError::TokenVerification)?;
    Ok(verified_data.into_vec())
}

/// Import a PGP private key that unlocks with an encrypted token from other keys.
///
/// Decrypts the encrypted token with the provided keys,
/// unlocks the imported key with the decrypted token, and verifies that signature over the token is valid.
pub fn import_key_with_token<Prov: PGPProviderSync>(
    provider: &Prov,
    private_key: &str,
    token: &str,
    signature: &str,
    decryption_keys: &[impl AsRef<Prov::PrivateKey>],
    verification_keys: &[impl AsPublicKeyRef<Prov::PublicKey>],
    verification_context: Option<Prov::VerificationContext>,
) -> Result<(Prov::PrivateKey, Prov::PublicKey), AccountCryptoError> {
    let decrypted_token = decrypt_key_token(
        provider,
        token,
        signature,
        decryption_keys,
        verification_keys,
        verification_context,
    )?;
    let private_key = provider
        .private_key_import(private_key.as_bytes(), decrypted_token, DataEncoding::Armor)
        .map_err(AccountCryptoError::KeyImport)?;
    let public_key = provider
        .private_key_to_public_key(&private_key)
        .map_err(AccountCryptoError::TransformPublic)?;
    Ok((private_key, public_key))
}

/// Decrypts an encrypted token.
pub async fn decrypt_key_token_async<Prov: PGPProviderAsync>(
    provider: &Prov,
    token: &str,
    signature: &str,
    decryption_keys: &[impl AsRef<Prov::PrivateKey>],
    verification_keys: &[impl AsPublicKeyRef<Prov::PublicKey>],
    verification_context: Option<Prov::VerificationContext>,
) -> Result<Vec<u8>, AccountCryptoError> {
    let mut decryptor = provider
        .new_decryptor_async()
        .with_decryption_key_refs(decryption_keys)
        .with_verification_key_refs(verification_keys)
        .with_detached_signature_ref(signature.as_bytes(), true);
    if let Some(context) = &verification_context {
        decryptor = decryptor.with_verification_context(context)
    }
    let verified_data = decryptor
        .decrypt_async(token.as_bytes(), DataEncoding::Armor)
        .await
        .map_err(AccountCryptoError::TokenDecryption)?;
    verified_data
        .verification_result()
        .map_err(AccountCryptoError::TokenVerification)?;
    Ok(verified_data.into_vec())
}

/// Import a PGP private key that unlocks with an encrypted token from other keys.
///
/// Decrypts the encrypted token with the provided keys,
/// unlocks the imported key with the decrypted token, and verifies that signature over the token is valid.
pub async fn import_key_with_token_async<Prov: PGPProviderAsync>(
    provider: &Prov,
    private_key: &str,
    token: &str,
    signature: &str,
    decryption_keys: &[impl AsRef<Prov::PrivateKey>],
    verification_keys: &[impl AsPublicKeyRef<Prov::PublicKey>],
    verification_context: Option<Prov::VerificationContext>,
) -> Result<(Prov::PrivateKey, Prov::PublicKey), AccountCryptoError> {
    let decrypted_token = decrypt_key_token_async(
        provider,
        token,
        signature,
        decryption_keys,
        verification_keys,
        verification_context,
    )
    .await?;
    let private_key = provider
        .private_key_import_async(private_key.as_bytes(), decrypted_token, DataEncoding::Armor)
        .await
        .map_err(AccountCryptoError::KeyImport)?;
    let public_key = provider
        .private_key_to_public_key_async(&private_key)
        .await
        .map_err(AccountCryptoError::TransformPublic)?;
    Ok((private_key, public_key))
}
