use crate::app_model::APP_ID;
use anyhow::anyhow;
use proton_core_common::os::{KeyChain, KeyChainEntryKind, KeyChainError, KeyChainExt};
use proton_mail_common::proton_core_common::db::account::SessionEncryptionKey;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;

pub struct AppKeyChain {
    session_key: Arc<keyring::Entry>,
    device_key: Arc<keyring::Entry>,
}

impl AppKeyChain {
    pub fn new() -> anyhow::Result<Self> {
        let session_key = keyring::Entry::new(APP_ID, "session_key")?;
        let device_key = keyring::Entry::new(APP_ID, "device_key")?;
        Ok(Self {
            session_key: Arc::new(session_key),
            device_key: Arc::new(device_key),
        })
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        let v = self.load::<SessionEncryptionKey>()?;
        if v.is_none() {
            let key = SessionEncryptionKey::random();
            self.store(key)?;
        }
        Ok(())
    }

    fn kind_to_entry(&self, kind: KeyChainEntryKind) -> &Arc<keyring::Entry> {
        match kind {
            KeyChainEntryKind::EncryptionKey => &self.session_key,
            KeyChainEntryKind::DeviceKey => &self.device_key,
            KeyChainEntryKind::PinHash => panic!("TUI does not support pin protection yet"),
        }
    }
}

impl KeyChain for AppKeyChain {
    fn store_entry(&self, kind: KeyChainEntryKind, key: SecretString) -> Result<(), KeyChainError> {
        self.kind_to_entry(kind)
            .set_password(key.expose_secret())
            .map_err(|e| KeyChainError::new(anyhow!(e).into()))?;
        Ok(())
    }

    fn delete_entry(&self, kind: KeyChainEntryKind) -> Result<(), KeyChainError> {
        if let Err(e) = self.kind_to_entry(kind).delete_credential()
            && !matches!(e, keyring::Error::NoEntry)
        {
            return Err(KeyChainError::new(anyhow!(e).into()));
        }
        Ok(())
    }

    fn load_entry(&self, kind: KeyChainEntryKind) -> Result<Option<SecretString>, KeyChainError> {
        match self.kind_to_entry(kind).get_password() {
            Ok(str) => Ok(Some(SecretString::new(str))),
            Err(e) => match e {
                keyring::Error::NoEntry => Ok(None),
                _ => Err(KeyChainError::new(anyhow!(e).into())),
            },
        }
    }
}
