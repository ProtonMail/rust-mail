use crate::session::CoreSessionError;
use proton_core_db::SessionEncryptionKey;
use std::error::Error;

pub trait KeyChain {
    fn store(&mut self, key: &[u8]) -> Result<(), Box<dyn Error>>;
    fn delete(&mut self) -> Result<(), Box<dyn Error>>;
    fn get(&self) -> Result<Option<Vec<u8>>, Box<dyn Error>>;

    fn new_session_key_chain(&self) -> Box<dyn SessionKeyChain>;
}

pub trait SessionKeyChain: Send + Sync {
    fn get_or_error(&self) -> Result<Vec<u8>, Box<dyn Error + Send>>;
}

pub(crate) fn session_encryption_key_from_key_chain(
    bytes: Vec<u8>,
) -> Result<SessionEncryptionKey, CoreSessionError> {
    SessionEncryptionKey::with_bytes(bytes).map_err(|mut v| {
        v.fill(0);
        CoreSessionError::Crypto
    })
}
