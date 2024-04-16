use proton_mail_common::proton_api_mail::proton_api_core::exports::anyhow::anyhow;
use proton_mail_common::proton_api_mail::proton_api_core::exports::thiserror;
use proton_mail_common::proton_core_common::os::{KeyChain, KeyChainError};

/// Errors for keychain operations.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum OSKeyChainError {
    /// OS operation failed.
    #[error("OS: {0}")]
    OS(String),
    /// Some other error occurred.
    #[error("Other: {0}")]
    Other(String),
}

/// Interface for accessing the OS keychain.
#[uniffi::export(callback_interface)]
pub trait OSKeyChain: Send + Sync {
    /// Store the secret in the keychain.
    fn store(&self, key: String) -> Result<(), OSKeyChainError>;

    /// Remote the secret from the keychain.
    fn delete(&self) -> Result<(), OSKeyChainError>;

    /// Retrieve the secret from the keychain.
    fn get(&self) -> Result<Option<String>, OSKeyChainError>;
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
        KeyChainError::from(anyhow!(self))
    }
}
impl KeyChain for FFIKeyChain {
    fn store(&self, key: String) -> Result<(), KeyChainError> {
        self.0.store(key).map_err(std::convert::Into::into)
    }

    fn delete(&self) -> Result<(), KeyChainError> {
        self.0.delete().map_err(std::convert::Into::into)
    }

    fn get(&self) -> Result<Option<String>, KeyChainError> {
        self.0.get().map_err(std::convert::Into::into)
    }
}
