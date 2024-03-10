use crate::state::APP_ID;
use anyhow::anyhow;
use proton_mail_common::proton_core_common::os::{KeyChain, KeyChainError};
use proton_mail_common::proton_core_common::proton_core_db::SessionEncryptionKey;
use secrecy::{ExposeSecret, SecretString};
use std::error::Error;
use std::sync::Arc;

pub struct AppKeyChain {
    entry: Arc<keyring::Entry>,
}

impl AppKeyChain {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let entry = keyring::Entry::new(APP_ID, "session_key")?;
        Ok(Self {
            entry: Arc::new(entry),
        })
    }

    pub fn init(&mut self) -> Result<(), Box<dyn Error>> {
        let v = self.get()?;
        if v.is_none() {
            let key = SessionEncryptionKey::random();
            self.store(key.to_base64())?;
        }
        Ok(())
    }
}

impl KeyChain for AppKeyChain {
    fn store(&self, key: String) -> Result<(), KeyChainError> {
        let key = SecretString::new(key);
        self.entry
            .set_password(key.expose_secret())
            .map_err(|e| KeyChainError::from(anyhow!(e)))?;
        Ok(())
    }

    fn delete(&self) -> Result<(), KeyChainError> {
        if let Err(e) = self.entry.delete_password() {
            if !matches!(e, keyring::Error::NoEntry) {
                return Err(KeyChainError::from(anyhow!(e)));
            }
        }
        Ok(())
    }

    fn get(&self) -> Result<Option<String>, KeyChainError> {
        match self.entry.get_password() {
            Ok(str) => Ok(Some(str)),
            Err(e) => match e {
                keyring::Error::NoEntry => Ok(None),
                _ => Err(KeyChainError::from(anyhow!(e))),
            },
        }
    }
}
