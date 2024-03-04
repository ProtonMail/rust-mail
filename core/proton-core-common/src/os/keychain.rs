use crate::session::CoreSessionError;
use proton_api_core::exports::parking_lot::Mutex;
use proton_core_db::SessionEncryptionKey;
use std::error::Error;

pub trait KeyChain: Send + Sync {
    fn store(&self, key: &[u8]) -> Result<(), Box<dyn Error>>;
    fn delete(&self) -> Result<(), Box<dyn Error>>;
    fn get(&self) -> Result<Option<Vec<u8>>, Box<dyn Error>>;
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
    fn store(&self, key: &[u8]) -> Result<(), Box<dyn Error>> {
        let mut guard = self.data.lock();
        if let Some(v) = guard.as_mut() {
            v.clear();
            v.extend_from_slice(key);
        } else {
            *guard = Some(Vec::from(key));
        }
        Ok(())
    }

    fn delete(&self) -> Result<(), Box<dyn Error>> {
        *self.data.lock() = None;
        Ok(())
    }

    fn get(&self) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let data = self.data.lock().clone();
        Ok(data)
    }
}
