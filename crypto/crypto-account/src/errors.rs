use proton_crypto::{crypto::VerificationError, CryptoError};

use crate::domain::KeyId;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("Could not unlock key with passphrase {0}:{1}")]
    Unlock(KeyId, AccountCryptoError),
    #[error("Could not unlock key with token {0}:{1}")]
    UnlockToken(KeyId, AccountCryptoError),
    #[error("Missing encryption token, signature, or flags for key {0}")]
    MissingValue(KeyId),
}

#[derive(Debug, thiserror::Error)]
pub enum AccountCryptoError {
    #[error("Failed to verify signature for token {0}")]
    TokenVerification(#[from] VerificationError),
    #[error("Failed to decrypt token {0}")]
    TokenDecryption(CryptoError),
    #[error("Failed to import key {0}")]
    KeyImport(CryptoError),
    #[error("Failed to export public key from private key {0}")]
    TransformPublic(CryptoError),
}

#[derive(Debug, thiserror::Error)]
pub enum SKLError {
    #[error("Failed to parse the SKL data: {0}")]
    ParseError(Box<dyn std::error::Error>),
    #[error("Failed to verify SKL signature: {0}")]
    SignatureVerificationError(#[from] VerificationError),
    #[error("No SKL data present")]
    NoSKLData,
}
