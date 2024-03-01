use aes_gcm::aead::consts::U12;
use aes_gcm::aead::Nonce;
use aes_gcm::aes::Aes256;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, AesGcm, Key, KeySizeUser,
};
use proton_api_core::auth::AuthScope;
use proton_api_core::domain::{ExposeSecret, SecretString, Uid, UserId};
use proton_api_core::exports::thiserror;
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::string::FromUtf8Error;
use zeroize::Zeroize;

pub struct DecryptedUserSession {
    pub session_id: Uid,
    pub user_id: UserId,
    pub name: Option<String>,
    pub email: String,
    pub refresh_token: SecretString,
    pub access_token: SecretString,
    pub scopes: Option<AuthScope>,
}

impl DecryptedUserSession {
    pub fn to_encrypted_session(
        &self,
        key: &SessionEncryptionKey,
    ) -> Result<EncryptedUserSession, aes_gcm::Error> {
        let encrypted_access_token = key.encrypt(self.access_token.expose_secret().as_bytes())?;
        let encrypted_refresh_token = key.encrypt(self.refresh_token.expose_secret().as_bytes())?;

        Ok(EncryptedUserSession {
            session_id: self.session_id.clone(),
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            email: self.email.clone(),
            refresh_token: encrypted_refresh_token,
            access_token: encrypted_access_token,
            scopes: self.scopes.clone(),
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EncryptedUserSession {
    pub session_id: Uid,
    pub user_id: UserId,
    pub name: Option<String>,
    pub email: String,
    pub refresh_token: EncryptedData,
    pub access_token: EncryptedData,
    pub scopes: Option<AuthScope>,
}

impl EncryptedUserSession {
    pub fn to_decrypted_session(
        &self,
        key: &SessionEncryptionKey,
    ) -> Result<DecryptedUserSession, DecryptionError> {
        let decrypted_access_token = key
            .decrypt(&self.access_token)
            .map_err(|_| DecryptionError::Decryption)?;
        let decrypted_access_token = SecretString::new(String::from_utf8(decrypted_access_token)?);

        let decrypted_refresh_token = key
            .decrypt(&self.refresh_token)
            .map_err(|_| DecryptionError::Decryption)?;
        let decrypted_refresh_token =
            SecretString::new(String::from_utf8(decrypted_refresh_token)?);

        Ok(DecryptedUserSession {
            session_id: self.session_id.clone(),
            user_id: self.user_id.clone(),
            name: self.name.clone(),
            email: self.email.clone(),
            refresh_token: decrypted_refresh_token,
            access_token: decrypted_access_token,
            scopes: self.scopes.clone(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecryptionError {
    #[error("Decryption failed")]
    Decryption,
    #[error("String Conversion: {0}")]
    String(#[from] FromUtf8Error),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EncryptedData {
    ciphertext_nonce: Vec<u8>,
}

impl AsRef<[u8]> for EncryptedData {
    fn as_ref(&self) -> &[u8] {
        &self.ciphertext_nonce
    }
}

impl ToSql for EncryptedData {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        self.ciphertext_nonce.to_sql()
    }
}

impl FromSql for EncryptedData {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Vec::<u8>::column_result(value).map(|v| Self {
            ciphertext_nonce: v,
        })
    }
}

#[derive(Clone)]
pub struct SessionEncryptionKey {
    key: Key<Aes256Gcm>,
}

impl Drop for SessionEncryptionKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

impl AsRef<[u8]> for SessionEncryptionKey {
    fn as_ref(&self) -> &[u8] {
        self.key.as_ref()
    }
}

impl SessionEncryptionKey {
    pub fn random() -> Self {
        let key = Aes256Gcm::generate_key(OsRng);
        Self { key }
    }

    pub fn with_bytes(mut bytes: Vec<u8>) -> Result<Self, Vec<u8>> {
        if bytes.len() < Aes256Gcm::key_size() {
            return Err(bytes);
        }
        let k = SessionEncryptionKey {
            key: Key::<Aes256Gcm>::clone_from_slice(&bytes),
        };
        bytes.zeroize();
        Ok(k)
    }

    pub fn encrypt(&self, data: &[u8]) -> Result<EncryptedData, aes_gcm::Error> {
        let cipher = Aes256Gcm::new(&self.key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut output = cipher.encrypt(&nonce, data)?;
        output.extend_from_slice(&nonce);
        Ok(EncryptedData {
            ciphertext_nonce: output,
        })
    }

    pub fn decrypt(&self, data: &EncryptedData) -> Result<Vec<u8>, aes_gcm::Error> {
        const NONCE_LENGTH: usize = 12;
        if data.ciphertext_nonce.len() < NONCE_LENGTH {
            return Err(aes_gcm::Error);
        }
        let cipher_text_size = data.ciphertext_nonce.len() - NONCE_LENGTH;
        let cipher = Aes256Gcm::new(&self.key);
        let nonce =
            Nonce::<AesGcm<Aes256, U12>>::from_slice(&data.ciphertext_nonce[cipher_text_size..]);
        debug_assert_eq!(nonce.len(), NONCE_LENGTH);
        let ciphertext = &data.ciphertext_nonce[0..cipher_text_size];
        let plain_text = cipher.decrypt(nonce, ciphertext)?;
        Ok(plain_text)
    }
}
