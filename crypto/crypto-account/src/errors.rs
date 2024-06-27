use std::io::Error;
use std::string::FromUtf8Error;

use proton_crypto::{crypto::VerificationError, CryptoError};

use crate::keys::KeyId;

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
    #[error("Failed to generate a fresh key {0}")]
    GenerateKey(CryptoError),
    #[error("Failed to armor key")]
    GenerateKeyArmor,
    #[error("Failed to encrypt token {0}")]
    TokenEncryption(CryptoError),
    #[error("Failed to encode token {0}")]
    TokenEncoding(#[from] FromUtf8Error),
    #[error("Found a legacy key when expecting no legacy key")]
    UnexpectedLegacy,
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

#[derive(Debug, thiserror::Error)]
pub enum CardCryptoError {
    #[error("Error decrypting card: {0}")]
    DecryptionError(CryptoError),
    #[error("Error encrypting card: {0}")]
    EncryptionError(CryptoError),
    #[error("Error signing card: {0}")]
    SigningError(CryptoError),
    #[error("Error writing card data to stream: {0}")]
    WriteError(Error),
    #[error("Error encoding data to string: {0}")]
    EncodingError(FromUtf8Error),
    #[error("Error verifying card signature: {0}")]
    SignatureVerificationError(#[from] VerificationError),
    #[error("No signature found for a signed card")]
    NoSignature,
}
