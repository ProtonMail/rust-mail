use anyhow::Result;
use proton_core_api::services::proton::muon::util::BoxErrExt;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{KeyChain, KeyChainEntryKind, KeyChainError};
use secrecy::{ExposeSecret, SecretString};
use std::fs;
use std::path::{Path, PathBuf};

pub struct OnDiskKeyChain {
    path: PathBuf,
}

impl OnDiskKeyChain {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().join("kch");

        if fs::exists(&path)? {
            info!("reusing existing keychain directory: {}", path.display());
        } else {
            fs::write(&path, SessionEncryptionKey::random().to_base64())?;
        };

        Ok(Self { path })
    }
}

impl KeyChain for OnDiskKeyChain {
    fn store_entry(&self, _: KeyChainEntryKind, key: SecretString) -> Result<(), KeyChainError> {
        fs::write(&self.path, key.expose_secret().as_bytes()).box_map_err(KeyChainError::new)?;

        Ok(())
    }

    fn delete_entry(&self, _: KeyChainEntryKind) -> Result<(), KeyChainError> {
        fs::remove_file(&self.path).box_map_err(KeyChainError::new)?;

        Ok(())
    }

    fn load_entry(&self, _: KeyChainEntryKind) -> Result<Option<SecretString>, KeyChainError> {
        let Ok(true) = fs::exists(&self.path) else {
            return Ok(None);
        };

        let entry = fs::read_to_string(&self.path)
            .map(SecretString::new)
            .box_map_err(KeyChainError::new)?;

        Ok(Some(entry))
    }
}
