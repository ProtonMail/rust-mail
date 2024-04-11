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

/// OS Keychain abstraction.
pub trait KeyChain: Send + Sync {
    /// Store the string encoded encryption key into the keychain.
    ///
    /// # Errors
    /// Should return error if the operation failed.
    fn store(&self, key: String) -> Result<(), KeyChainError>;

    /// Delete the encryption key from the keychain.
    ///
    /// # Errors
    /// Should return error if the operation failed.
    fn delete(&self) -> Result<(), KeyChainError>;

    /// Retrieve the encryption key from the keychain. Should return `None` if it does not exist.
    ///
    /// # Errors
    /// Should return error if the operation failed.
    fn get(&self) -> Result<Option<String>, KeyChainError>;
}

#[derive(Default)]
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
