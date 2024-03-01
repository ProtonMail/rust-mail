use crate::state::APP_ID;
use anyhow::anyhow;
use hex::FromHexError;
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
            self.store(key.as_ref())?;
        }
        Ok(())
    }
}

impl KeyChain for AppKeyChain {
    fn store(&self, key: &[u8]) -> Result<(), KeyChainError> {
        let hex_str = bytes_to_hex(key);
        self.entry
            .set_password(&hex_str)
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

    fn get(&self) -> Result<Option<Vec<u8>>, KeyChainError> {
        match self.entry.get_password() {
            Ok(hex_str) => {
                let hex_str = SecretString::new(hex_str);
                Ok(Some(
                    hex_str_to_bytes(hex_str.expose_secret().as_str())
                        .map_err(|e| KeyChainError::from(anyhow!(e)))?,
                ))
            }
            Err(e) => match e {
                keyring::Error::NoEntry => Ok(None),
                _ => Err(KeyChainError::from(anyhow!(e))),
            },
        }
    }
}

fn bytes_to_hex(b: &[u8]) -> String {
    hex::encode(b)
}

fn hex_str_to_bytes(str: &str) -> Result<Vec<u8>, FromHexError> {
    hex::decode(str)
}
