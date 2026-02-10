use proton_crypto_account::proton_crypto::CryptoError;

mod verification;
pub use verification::*;

mod encryption;
pub use encryption::*;

mod session_key;
pub use session_key::*;

mod errors;
pub use errors::*;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum SessionKeyError {
    #[error("Invalid session key: {0}")]
    InvalidSessionKey(String),
    #[error("Failed to import key with the OpenPGP provider: {0}")]
    Import(CryptoError),
    #[error("Failed to import key with the OpenPGP provider: {0}")]
    KeyPacketEncryption(CryptoError),
}
