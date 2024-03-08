use proton_api_core::exports::parking_lot::Mutex;
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
    fn store(&self, key: String) -> Result<(), KeyChainError>;
    fn delete(&self) -> Result<(), KeyChainError>;
    fn get(&self) -> Result<Option<String>, KeyChainError>;
}

pub struct InMemoryKeyChain {
    data: Mutex<Option<String>>,
}

impl KeyChain for InMemoryKeyChain {
    fn store(&self, key: String) -> Result<(), KeyChainError> {
        let mut guard = self.data.lock();
        *guard = Some(key);
        Ok(())
    }

    fn delete(&self) -> Result<(), KeyChainError> {
        *self.data.lock() = None;
        Ok(())
    }

    fn get(&self) -> Result<Option<String>, KeyChainError> {
        let data = self.data.lock().clone();
        Ok(data)
    }
}
