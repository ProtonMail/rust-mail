use parking_lot::Mutex;
use secrecy::SecretString;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Formatter;
use std::sync::Arc;

#[derive(Debug)]
pub struct KeyChainError(Box<dyn Error + Send + Sync>);

impl KeyChainError {
    /// Create new instance.
    #[must_use]
    pub fn new(e: Box<dyn Error + Send + Sync>) -> Self {
        //Note: Can't use from as it conflicts with exiting from errors.
        Self(e)
    }
}

impl std::fmt::Display for KeyChainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for KeyChainError {}

/// What is the kind of the data. OS key chains might support multiple
/// entries. This enum might be seen as a key in a `HashMap`.
///
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyChainEntryKind {
    /// Session key used to encrypt and decrypt sensitive data
    ///
    EncryptionKey,

    /// Shared key between all accounts, used to decrypt push notifications
    ///
    DeviceKey,

    /// Pin hash protection
    ///
    PinHash,
}

/// A type that can be stored in the [`KeyChain`]
///
pub trait StoreInKeyChain: Sized {
    /// What is the kind of the data. OS key chains might support multiple
    /// entries. This method helps picking the right entry.
    ///
    fn kind() -> KeyChainEntryKind;
    /// Used to load entry from plaintext
    ///
    fn from_stored_string(s: SecretString) -> Result<Self, Box<dyn Error + Send + Sync>>;
    /// Used to store entry in the plaintext
    ///
    fn to_stored_string(&self) -> SecretString;
}

/// OS Keychain abstraction.
pub trait KeyChain: Send + Sync {
    /// Store the string encoded encryption key into the keychain.
    fn store_entry(&self, kind: KeyChainEntryKind, key: SecretString) -> Result<(), KeyChainError>;

    /// Delete the encryption key from the keychain.
    fn delete_entry(&self, kind: KeyChainEntryKind) -> Result<(), KeyChainError>;

    /// Retrieve the encryption key from the keychain. Should return `None` if it does not exist.
    fn load_entry(&self, kind: KeyChainEntryKind) -> Result<Option<SecretString>, KeyChainError>;
}

/// This is an extension trait over [`KeyChain`] to prevent "cannot be made dyn-compatible"
/// errors.
/// It is implemented automatically
pub trait KeyChainExt: KeyChain {
    /// Store the string encoded encryption key into the keychain.
    fn store<T: StoreInKeyChain>(&self, key: T) -> Result<(), KeyChainError> {
        self.store_entry(T::kind(), key.to_stored_string())
    }

    /// Delete the encryption key from the keychain.
    fn delete<T: StoreInKeyChain>(&self) -> Result<(), KeyChainError> {
        self.delete_entry(T::kind())
    }

    /// Retrieve the encryption key from the keychain. Should return `None` if it does not exist.
    fn load<T: StoreInKeyChain>(&self) -> Result<Option<T>, KeyChainError> {
        let entry = self.load_entry(T::kind())?;
        entry
            .map(StoreInKeyChain::from_stored_string)
            .transpose()
            .map_err(KeyChainError)
    }
}

impl KeyChain for Arc<dyn KeyChain> {
    fn store_entry(&self, kind: KeyChainEntryKind, key: SecretString) -> Result<(), KeyChainError> {
        (**self).store_entry(kind, key)
    }

    fn delete_entry(&self, kind: KeyChainEntryKind) -> Result<(), KeyChainError> {
        (**self).delete_entry(kind)
    }

    fn load_entry(&self, kind: KeyChainEntryKind) -> Result<Option<SecretString>, KeyChainError> {
        (**self).load_entry(kind)
    }
}

impl<T: KeyChain> KeyChainExt for T {}

#[derive(Default)]
pub struct InMemoryKeyChain {
    data: Mutex<HashMap<KeyChainEntryKind, SecretString>>,
}

impl KeyChain for InMemoryKeyChain {
    fn store_entry(&self, kind: KeyChainEntryKind, key: SecretString) -> Result<(), KeyChainError> {
        let mut guard = self.data.lock();
        guard.insert(kind, key);
        Ok(())
    }

    fn delete_entry(&self, kind: KeyChainEntryKind) -> Result<(), KeyChainError> {
        self.data.lock().remove(&kind);
        Ok(())
    }

    fn load_entry(&self, kind: KeyChainEntryKind) -> Result<Option<SecretString>, KeyChainError> {
        let data = self.data.lock().get(&kind).cloned();
        Ok(data)
    }
}
