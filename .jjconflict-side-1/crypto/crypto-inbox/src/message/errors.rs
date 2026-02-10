use std::{str::Utf8Error, string::FromUtf8Error};

use proton_crypto_account::proton_crypto::CryptoError;
use proton_crypto_inbox_mime::ProcessMimeError;

use crate::keys::SessionKeyError;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum MessageError {
    #[error("Failed to encrypt the message body: {0}")]
    Encryption(CryptoError),
    #[error("Failed to decrypt the message body: {0}")]
    Decryption(CryptoError),
    #[error("Message import failed: {0}")]
    ImportProblem(CryptoError),
    #[error("Failed to decode message body to utf-8 string: {0}")]
    BodyDecode(#[from] Utf8Error),
    #[error("Failed to decode mime message body: {0}")]
    MimeBodyDecode(#[from] ProcessMimeError),
    #[error("Missing message identifier for mime decryption")]
    MissingMessageID,
    #[error("Session key encryption failed: {0}")]
    SessionKeyEncryption(#[from] SessionKeyError),
    #[error("Failed to decode message body to an UTF-8 string: {0}")]
    StringEncode(#[from] FromUtf8Error),
}
