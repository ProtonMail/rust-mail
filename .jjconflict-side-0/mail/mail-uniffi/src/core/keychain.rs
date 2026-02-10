use proton_core_common::os::KeyChainEntryKind;
use proton_core_common::os::{KeyChain, KeyChainError};
use secrecy::{ExposeSecret, SecretString};

/// Errors for keychain operations.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum OSKeyChainError {
    /// OS operation failed.
    #[error("OS: {0}")]
    OS(String),
    /// Some other error occurred.
    #[error("Other: {0}")]
    Other(String),
}

impl From<uniffi::UnexpectedUniFFICallbackError> for OSKeyChainError {
    fn from(value: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Other(value.to_string())
    }
}

/// Interface for accessing the OS keychain.
#[uniffi::export(callback_interface)]
pub trait OSKeyChain: Send + Sync {
    /// Store the secret in the keychain.
    fn store(&self, kind: OSKeyChainEntryKind, key: String) -> Result<(), OSKeyChainError>;

    /// Remote the secret from the keychain.
    fn delete(&self, kind: OSKeyChainEntryKind) -> Result<(), OSKeyChainError>;

    /// Retrieve the secret from the keychain.
    fn load(&self, kind: OSKeyChainEntryKind) -> Result<Option<String>, OSKeyChainError>;
}

/// What is the kind of the data. OS key chains might support multiple
/// entries. This enum might be seen as a key in a `HashMap`.
///
#[derive(uniffi::Enum)]
pub enum OSKeyChainEntryKind {
    /// Session key used to encrypt and decrypt sensitive data
    ///
    EncryptionKey,
    /// Shared key between all accounts, used to decrypt push notifications
    ///
    DeviceKey,

    /// App protection - PIN hash
    ///
    PinHash,
}

impl From<KeyChainEntryKind> for OSKeyChainEntryKind {
    fn from(value: KeyChainEntryKind) -> Self {
        match value {
            KeyChainEntryKind::EncryptionKey => Self::EncryptionKey,
            KeyChainEntryKind::DeviceKey => Self::DeviceKey,
            KeyChainEntryKind::PinHash => Self::PinHash,
        }
    }
}

pub(crate) struct FFIKeyChain(pub(crate) Box<dyn OSKeyChain>);

impl From<Box<dyn OSKeyChain>> for FFIKeyChain {
    fn from(value: Box<dyn OSKeyChain>) -> Self {
        Self(value)
    }
}

#[allow(clippy::from_over_into)] // we don't want conversions the other way.
impl Into<KeyChainError> for OSKeyChainError {
    fn into(self) -> KeyChainError {
        KeyChainError::new(self.into())
    }
}
impl KeyChain for FFIKeyChain {
    fn store_entry(&self, kind: KeyChainEntryKind, key: SecretString) -> Result<(), KeyChainError> {
        let kind = kind.into();
        self.0
            .store(kind, key.expose_secret().clone())
            .map_err(Into::into)
    }

    fn delete_entry(&self, kind: KeyChainEntryKind) -> Result<(), KeyChainError> {
        let kind = kind.into();
        self.0.delete(kind).map_err(Into::into)
    }

    fn load_entry(&self, kind: KeyChainEntryKind) -> Result<Option<SecretString>, KeyChainError> {
        let kind = kind.into();
        self.0
            .load(kind)
            .map_err(Into::into)
            .map(|o| o.map(SecretString::new))
    }
}
