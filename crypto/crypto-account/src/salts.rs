use crate::keys::KeyId;
use base64::Engine;
use proton_crypto::srp::SRPProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SaltError {
    #[error("Could not find key with id {0}")]
    KeyNotFound(KeyId),
    #[error("Key with id {0} has no salt value")]
    KeyHasNotSalt(KeyId),
    #[error("Could not decode key salt: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Failed to hash: {0}")]
    Hash(#[from] crate::Error),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Salt {
    #[serde(rename = "ID")]
    pub id: KeyId,
    #[serde(rename = "KeySalt")]
    pub key_salt: Option<String>,
}

pub struct SaltedPassword<T: AsRef<[u8]>>(T);

impl<T: AsRef<[u8]>> AsRef<[u8]> for SaltedPassword<T> {
    fn as_ref(&self) -> &[u8] {
        let r = self.0.as_ref();
        &r[r.len() - 31..]
    }
}

#[derive(Deserialize, Debug)]
pub struct Salts(Vec<Salt>);
impl Salts {
    pub fn new(salts: impl IntoIterator<Item = Salt>) -> Self {
        Self(salts.into_iter().collect::<Vec<_>>())
    }

    pub fn salt_for_key<T: SRPProvider>(
        &self,
        srp_provider: &T,
        key: &KeyId,
        key_pass: &[u8],
    ) -> Result<SaltedPassword<T::HashedPassword>, SaltError> {
        let Some(salt) = self.0.iter().find(|&v| v.id == *key) else {
            return Err(SaltError::KeyNotFound(key.clone()));
        };

        let Some(key_salt) = &salt.key_salt else {
            return Err(SaltError::KeyHasNotSalt(key.clone()));
        };

        let b64 = base64::engine::general_purpose::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::general_purpose::PAD,
        );

        let key_salt_decoded = b64.decode(key_salt)?;

        let result = srp_provider.mailbox_password(key_pass, key_salt_decoded)?;

        Ok(SaltedPassword(result))
    }
}
