mod verification;
use proton_crypto_account::proton_crypto::CryptoError;
pub use verification::*;

mod session_key;
pub use session_key::*;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum SessionKeyError {
    #[error("Invalid session key")]
    InvalidSessionKey,
    #[error("Failed to import key with the OpenPGP provider: {0}")]
    Import(CryptoError),
    #[error("Failed to import key with the OpenPGP provider: {0}")]
    KeyPacketEncryption(CryptoError),
}
