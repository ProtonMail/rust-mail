use crate::session::CoreSessionError;
use proton_api_core::exports::parking_lot::Mutex;
use proton_core_db::SessionEncryptionKey;
use std::error::Error;
use std::fmt::Formatter;

#[derive(Debug)]
pub struct KeyChainError(Box<dyn Error + Send>);

impl std::fmt::Display for KeyChainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for KeyChainError {}

impl<T: Into<Box<dyn Error + Send>>> From<T> for KeyChainError {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

pub trait KeyChain: Send + Sync {
    fn store(&self, key: &[u8]) -> Result<(), KeyChainError>;
    fn delete(&self) -> Result<(), KeyChainError>;
    fn get(&self) -> Result<Option<Vec<u8>>, KeyChainError>;
}

pub(crate) fn session_encryption_key_from_key_chain(
    bytes: Vec<u8>,
) -> Result<SessionEncryptionKey, CoreSessionError> {
    SessionEncryptionKey::with_bytes(bytes).map_err(|mut v| {
        v.fill(0);
        CoreSessionError::Crypto
    })
}

pub struct InMemoryKeyChain {
    data: Mutex<Option<Vec<u8>>>,
}

impl KeyChain for InMemoryKeyChain {
    fn store(&self, key: &[u8]) -> Result<(), KeyChainError> {
        let mut guard = self.data.lock();
        if let Some(v) = guard.as_mut() {
            v.clear();
            v.extend_from_slice(key);
        } else {
            *guard = Some(Vec::from(key));
        }
        Ok(())
    }

    fn delete(&self) -> Result<(), KeyChainError> {
        *self.data.lock() = None;
        Ok(())
    }

    fn get(&self) -> Result<Option<Vec<u8>>, KeyChainError> {
        let data = self.data.lock().clone();
        Ok(data)
    }
}
