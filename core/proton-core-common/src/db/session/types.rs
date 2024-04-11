use aes_gcm::aead::consts::U12;
use aes_gcm::aead::Nonce;
use aes_gcm::aes::Aes256;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, AesGcm, Key, KeySizeUser,
};
use proton_api_core::auth::{AccessToken, RefreshToken, Scope};
use proton_api_core::domain::{Uid, UserId};
use proton_api_core::exports::base64::prelude::BASE64_STANDARD;
use proton_api_core::exports::base64::Engine;
use proton_api_core::exports::thiserror;
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::string::FromUtf8Error;
use zeroize::Zeroize;

/// Contains the session authentication in a decrypted state, ready to be used by the
/// http client.
pub struct DecryptedUserSession {
    pub session_id: Uid,
    pub user_id: UserId,
    pub name: Option<String>,
    pub email: String,
    pub refresh_token: RefreshToken,
    pub access_token: AccessToken,
    pub scopes: Scope,
}

impl DecryptedUserSession {
    /// Encrypt the session data so that it can be stored securely.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn to_encrypted_session(
        &self,
        key: &SessionEncryptionKey,
    ) -> Result<EncryptedUserSession, aes_gcm::Error> {
        let encrypted_access_token = key
            .encrypt(self.access_token.expose_secret().as_bytes())
            .map(EncryptedAccessToken)?;
        let encrypted_refresh_token = key
            .encrypt(self.refresh_token.expose_secret().as_bytes())
            .map(EncryptedRefreshToken)?;

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

/// Encrypted session authentication data, can safely be stored on disk.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EncryptedUserSession {
    pub session_id: Uid,
    pub user_id: UserId,
    pub name: Option<String>,
    pub email: String,
    pub refresh_token: EncryptedRefreshToken,
    pub access_token: EncryptedAccessToken,
    pub scopes: Scope,
}

impl EncryptedUserSession {
    /// Decrypt the session data so that it can be used.
    ///
    /// # Errors
    /// Returns error if the decryption failed.
    pub fn to_decrypted_session(
        &self,
        key: &SessionEncryptionKey,
    ) -> Result<DecryptedUserSession, DecryptionError> {
        let decrypted_access_token = key
            .decrypt(&self.access_token.0)
            .map_err(|_| DecryptionError::Decryption)?;
        let decrypted_access_token = AccessToken::from(String::from_utf8(decrypted_access_token)?);

        let decrypted_refresh_token = key
            .decrypt(&self.refresh_token.0)
            .map_err(|_| DecryptionError::Decryption)?;
        let decrypted_refresh_token =
            RefreshToken::from(String::from_utf8(decrypted_refresh_token)?);

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

/// Encrypted Access token wrapper.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EncryptedAccessToken(pub(crate) EncryptedData);

impl EncryptedAccessToken {
    /// Encrypt the access token.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn new(token: &AccessToken, key: &SessionEncryptionKey) -> Result<Self, aes_gcm::Error> {
        key.encrypt(token.expose_secret().as_bytes()).map(Self)
    }
}
impl AsRef<[u8]> for EncryptedAccessToken {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// Encrypted refresh token wrapper.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EncryptedRefreshToken(pub(crate) EncryptedData);

impl EncryptedRefreshToken {
    /// Encrypt the refresh token.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn new(token: &RefreshToken, key: &SessionEncryptionKey) -> Result<Self, aes_gcm::Error> {
        key.encrypt(token.expose_secret().as_bytes()).map(Self)
    }
}
impl AsRef<[u8]> for EncryptedRefreshToken {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
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

//TODO: This could potentially be reused in other contexts.
/// Encryption key for encryption of session data.
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
    /// Create a new random encryption key.
    #[must_use]
    pub fn random() -> Self {
        let key = Aes256Gcm::generate_key(OsRng);
        Self { key }
    }

    /// Create a key from a collection of bytes.
    ///
    /// # Errors
    /// Return error if the len of the collection is invalid.
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

    /// Encrypt the data.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn encrypt(&self, data: &[u8]) -> Result<EncryptedData, aes_gcm::Error> {
        let cipher = Aes256Gcm::new(&self.key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut output = cipher.encrypt(&nonce, data)?;
        output.extend_from_slice(&nonce);
        Ok(EncryptedData {
            ciphertext_nonce: output,
        })
    }

    /// Decrypt the data.
    ///
    /// # Errors
    /// Returns errors if the decryption failed.

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

    /// Convert the key into a vector of bytes.
    #[must_use]
    pub fn to_vec(&self) -> Vec<u8> {
        self.key.to_vec()
    }

    /// Convert the key into a base64 encoded string.
    #[must_use]
    pub fn to_base64(&self) -> String {
        BASE64_STANDARD.encode(self.key)
    }

    /// Create a key from a base64 string.
    #[must_use]
    pub fn from_base64(value: &str) -> Option<Self> {
        let Ok(bytes) = BASE64_STANDARD.decode(value) else {
            return None;
        };

        let Ok(key) = Self::with_bytes(bytes) else {
            return None;
        };

        Some(key)
    }
}
