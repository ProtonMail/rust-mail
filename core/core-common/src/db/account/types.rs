#[cfg(test)]
#[path = "../../tests/db/types.rs"]
mod tests;

use crate::datatypes::{AccountDetails, AuthScopes, AvatarInformation, PasswordMode, TfaStatus};
use crate::models::ModelExtension;
use crate::os::StoreInKeyChain;
use aes_gcm::aead::Nonce;
use aes_gcm::aead::consts::U12;
use aes_gcm::aes::Aes256;
use aes_gcm::{
    Aes256Gcm, AesGcm, Key, KeySizeUser,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use derive_more::{AsRef, Deref};
use proton_core_api::auth::{Tokens, UserKeySecret};

use proton_core_api::services::proton::{SessionId, UserId};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sqlite_watcher::watcher::TableObserver;
use stash::exports::{FromSql, FromSqlResult, SqliteError, ToSql, ToSqlOutput, ValueRef};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Stash, StashError, Tether, WatcherHandle};
use stash::{params, sql_using_serde};
use std::collections::{BTreeSet, HashSet};
use std::ops::Deref;
use std::string::FromUtf8Error;
use thiserror::Error;
use zeroize::Zeroize;

#[derive(Debug, Clone, PartialEq, Model)]
#[TableName("core_accounts")]
pub struct CoreAccount {
    #[IdField]
    pub remote_id: UserId,

    #[DbField]
    pub name_or_addr: String,

    #[DbField]
    pub second_factor_mode: Option<TfaStatus>,

    #[DbField]
    pub password_mode: Option<PasswordMode>,

    #[DbField]
    pub username: Option<String>,

    #[DbField]
    pub password: Option<EncryptedPassword>,

    #[DbField]
    pub display_name: Option<String>,

    #[DbField]
    pub primary_addr: Option<String>,

    #[DbField]
    pub primary_seq: i64,

    #[DbField]
    pub temp_pass: bool,

    #[DbField]
    pub is_ready: bool,
}

impl CoreAccount {
    #[must_use]
    pub fn new(remote_id: UserId, name_or_addr: String) -> Self {
        Self {
            remote_id,
            name_or_addr,
            primary_seq: 0,
            is_ready: false,
            username: None,
            password: None,
            display_name: None,
            primary_addr: None,
            second_factor_mode: None,
            password_mode: None,
            temp_pass: false,
        }
    }

    pub async fn by_primary_seq(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find("ORDER BY primary_seq DESC", vec![], tether).await
    }

    pub async fn primary_seq_max(tether: &Tether) -> Result<i64, StashError> {
        let query = format!("SELECT MAX(primary_seq) FROM {}", Self::table_name());

        tether.query_value(query, vec![]).await
    }

    #[must_use]
    pub fn with_username(self, username: String) -> Self {
        Self {
            username: Some(username),
            ..self
        }
    }

    #[must_use]
    pub fn with_name_or_addr(self, name_or_addr: String) -> Self {
        Self {
            name_or_addr,
            ..self
        }
    }

    pub fn with_password(
        self,
        pass: &str,
        key: &SessionEncryptionKey,
    ) -> Result<Self, CoreSessionError> {
        Ok(Self {
            password: Some(EncryptedPassword::new(pass, key)?),
            ..self
        })
    }

    #[must_use]
    pub fn without_password(self) -> Self {
        Self {
            password: None,
            ..self
        }
    }

    #[must_use]
    pub fn with_display_name(self, display_name: String) -> Self {
        Self {
            display_name: Some(display_name),
            ..self
        }
    }

    #[must_use]
    pub fn with_primary_addr(self, primary_addr: String) -> Self {
        Self {
            primary_addr: Some(primary_addr),
            ..self
        }
    }

    #[must_use]
    pub fn with_tfa_mode(self, mode: TfaStatus) -> Self {
        Self {
            second_factor_mode: Some(mode),
            ..self
        }
    }

    #[must_use]
    pub fn with_mbp_mode(self, mode: PasswordMode) -> Self {
        Self {
            password_mode: Some(mode),
            ..self
        }
    }

    #[must_use]
    pub fn with_primary_seq(self, primary_seq: i64) -> Self {
        Self {
            primary_seq,
            ..self
        }
    }

    #[must_use]
    pub fn with_temp_pass(self, value: bool) -> Self {
        Self {
            temp_pass: value,
            ..self
        }
    }

    #[must_use]
    pub fn with_ready(self) -> Self {
        Self {
            is_ready: true,
            ..self
        }
    }

    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(CoreAccountWatcher { sender }))
            .await
    }

    #[must_use]
    pub fn details(&self) -> AccountDetails {
        let name = self
            .display_name
            .clone()
            .filter(|name| !name.is_empty())
            .or(self.username.clone())
            .filter(|username| !username.is_empty())
            .unwrap_or_else(|| self.name_or_addr.clone());

        let email = self
            .primary_addr
            .clone()
            .unwrap_or_else(|| self.name_or_addr.clone());

        let avatar_information = AvatarInformation::from(name.clone());

        AccountDetails {
            name,
            email,
            avatar_information,
        }
    }
}

pub struct CoreAccountWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for CoreAccountWatcher {
    fn tables(&self) -> Vec<String> {
        vec![CoreAccount::table_name().to_string()]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for CoreAccountWatcher: {:?}",
                    e
                );
            })
            .ok();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Model)]
#[TableName("core_sessions")]
pub struct CoreSession {
    #[IdField]
    pub remote_id: SessionId,

    #[DbField]
    pub account_id: UserId,

    #[DbField]
    pub access_token: EncryptedAccessToken,

    #[DbField]
    pub refresh_token: EncryptedRefreshToken,

    #[DbField]
    pub auth_scopes: AuthScopes,

    #[DbField]
    pub key_secret: Option<EncryptedKeySecret>,
}

#[derive(Debug, Error)]
pub enum CoreSessionError {
    #[error("missing access token")]
    AccTok,

    #[error("missing auth scopes")]
    Scopes,

    #[error("AES GCM error: {0}")]
    AesGcm(#[from] aes_gcm::Error),
}

impl CoreSession {
    /// Retrieves all sessions associated with the given account ID.
    ///
    /// # Errors
    ///
    /// Returns error if we fail to retrieve the sessions from the db.
    pub async fn find_by_user_id(
        user_id: UserId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Self::find("WHERE account_id = ?", params![user_id], tether).await
    }

    /// Create a new session for the given account.
    ///
    /// # Errors
    ///
    /// Returns an error if the encryption fails.
    pub fn new(
        user_id: UserId,
        session_id: SessionId,
        tokens: &Tokens,
        key: &SessionEncryptionKey,
    ) -> Result<Self, CoreSessionError> {
        let ref_tok = tokens.ref_tok();
        let acc_tok = tokens.acc_tok().ok_or(CoreSessionError::AccTok)?;
        let scopes = tokens.scopes().ok_or(CoreSessionError::Scopes)?;

        Ok(Self {
            remote_id: session_id,
            account_id: user_id,
            access_token: EncryptedAccessToken::new(acc_tok, key)?,
            refresh_token: EncryptedRefreshToken::new(ref_tok, key)?,
            auth_scopes: AuthScopes::new(scopes),

            // --- Optional fields ---
            key_secret: None,
        })
    }

    /// Update the auth tokens.
    ///
    /// # Errors
    ///
    /// Returns an error if the encryption fails.
    ///
    pub fn with_tokens(
        self,
        tokens: &Tokens,
        key: &SessionEncryptionKey,
    ) -> Result<Self, CoreSessionError> {
        let ref_tok = tokens.ref_tok();
        let acc_tok = tokens.acc_tok().ok_or(CoreSessionError::AccTok)?;
        let scopes = tokens.scopes().ok_or(CoreSessionError::Scopes)?;

        Ok(Self {
            access_token: EncryptedAccessToken::new(acc_tok, key)?,
            refresh_token: EncryptedRefreshToken::new(ref_tok, key)?,
            auth_scopes: AuthScopes::new(scopes),

            // --- preserve ---
            ..self
        })
    }

    /// Update the key secret.
    ///
    /// # Errors
    ///
    /// Returns an error if the secret encryption fails.
    pub fn with_key_secret(
        self,
        key_secret: &UserKeySecret,
        key: &SessionEncryptionKey,
    ) -> Result<Self, aes_gcm::Error> {
        Ok(Self {
            key_secret: Some(EncryptedKeySecret::new(key_secret, key)?),

            // --- preserve ---
            ..self
        })
    }

    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(CoreSessionWatcher { sender }))
            .await
    }
}

pub struct CoreSessionWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for CoreSessionWatcher {
    fn tables(&self) -> Vec<String> {
        vec![CoreSession::table_name().to_string()]
    }
    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for CoreSessionWatcher: {:?}",
                    e
                );
            })
            .ok();
    }
}

#[derive(Debug, Error)]
pub enum DecryptionError {
    #[error("String Conversion: {0}")]
    String(#[from] FromUtf8Error),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptedData {
    ciphertext_nonce: Vec<u8>,
}

/// Encrypted Access token wrapper.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptedAccessToken(pub(crate) EncryptedData);

impl EncryptedAccessToken {
    /// Encrypt the access token.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn new(token: &str, key: &SessionEncryptionKey) -> Result<Self, aes_gcm::Error> {
        key.encrypt(token.as_bytes()).map(Self)
    }
}
impl Deref for EncryptedAccessToken {
    type Target = EncryptedData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl AsRef<[u8]> for EncryptedAccessToken {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl FromSql for EncryptedAccessToken {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        Ok(Self(EncryptedData {
            ciphertext_nonce: Vec::<u8>::column_result(value)?,
        }))
    }
}
impl ToSql for EncryptedAccessToken {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

/// Encrypted refresh token wrapper.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptedRefreshToken(pub(crate) EncryptedData);

impl EncryptedRefreshToken {
    /// Encrypt the refresh token.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn new(token: &str, key: &SessionEncryptionKey) -> Result<Self, aes_gcm::Error> {
        key.encrypt(token.as_bytes()).map(Self)
    }
}

impl Deref for EncryptedRefreshToken {
    type Target = EncryptedData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl AsRef<[u8]> for EncryptedRefreshToken {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl FromSql for EncryptedRefreshToken {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        Ok(Self(EncryptedData {
            ciphertext_nonce: Vec::<u8>::column_result(value)?,
        }))
    }
}
impl ToSql for EncryptedRefreshToken {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

impl AsRef<[u8]> for EncryptedData {
    fn as_ref(&self) -> &[u8] {
        &self.ciphertext_nonce
    }
}

impl ToSql for EncryptedData {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
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

/// Encrypted key secret wrapper.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptedKeySecret(pub(crate) EncryptedData);

impl EncryptedKeySecret {
    /// Encrypt the key secret.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn new(
        key_secret: &UserKeySecret,
        key: &SessionEncryptionKey,
    ) -> Result<Self, aes_gcm::Error> {
        key.encrypt(key_secret.expose_secret().as_bytes()).map(Self)
    }
}

impl Deref for EncryptedKeySecret {
    type Target = EncryptedData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for EncryptedKeySecret {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

sql_using_serde!(EncryptedKeySecret);

/// Encrypted account password wrapper.
#[derive(Clone, Debug, AsRef, Deref, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptedPassword(pub(crate) EncryptedData);

impl EncryptedPassword {
    /// Encrypt the password.
    ///
    /// # Errors
    /// Returns error if the encryption failed.
    pub fn new(password: &str, key: &SessionEncryptionKey) -> Result<Self, aes_gcm::Error> {
        key.encrypt(password.as_bytes()).map(Self)
    }
}

impl ToSql for EncryptedPassword {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        self.0.to_sql()
    }
}

impl FromSql for EncryptedPassword {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        Ok(Self(EncryptedData {
            ciphertext_nonce: Vec::<u8>::column_result(value)?,
        }))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("The length of the key is invalid")]
pub struct InvalidLengthOfSessionKey;

//TODO: This could potentially be reused in other contexts.
/// Encryption key for encryption of session data.
/// The key used to decrypt database secrets. It is not the User's passphrase
#[derive(Clone)]
pub struct SessionEncryptionKey {
    key: Key<Aes256Gcm>,
}

impl StoreInKeyChain for SessionEncryptionKey {
    fn kind() -> crate::os::KeyChainEntryKind {
        crate::os::KeyChainEntryKind::EncryptionKey
    }
    fn from_stored_string(
        s: SecretString,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::try_from_base64(s.expose_secret())
    }

    fn to_stored_string(&self) -> SecretString {
        SecretString::new(self.to_base64())
    }
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
    pub fn decrypt<D>(&self, data: D) -> Result<Vec<u8>, aes_gcm::Error>
    where
        D: Deref<Target = EncryptedData>,
    {
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
        Self::try_from_base64(value).ok()
    }

    /// Tries to create a key from base64.
    /// Comparing to [`Self::from_base64`] it does not return an option, but
    /// a proper error.
    pub fn try_from_base64(value: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = BASE64_STANDARD.decode(value)?;

        let key = Self::with_bytes(bytes).map_err(|_| InvalidLengthOfSessionKey)?;

        Ok(key)
    }
}

#[derive(Debug, Clone)]
pub enum CoreSessionObserverNotification {
    Created(SessionId, UserId),
    Deleted(SessionId, UserId),
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct CoreSessionObserverValue {
    session_id: SessionId,
    user_id: UserId,
}

impl From<CoreSession> for CoreSessionObserverValue {
    fn from(value: CoreSession) -> Self {
        Self {
            session_id: value.remote_id,
            user_id: value.account_id,
        }
    }
}

/// This observer only issues a series of notifications when changes occur in the session table.
pub struct CoreSessionObserver {
    sessions: HashSet<CoreSessionObserverValue>,
    stash: Stash,
    watcher: WatcherHandle,
}

impl CoreSessionObserver {
    pub async fn new(stash: Stash) -> Result<Self, StashError> {
        let tether = stash.connection().await?;
        let existing = CoreSession::all(&tether)
            .await?
            .into_iter()
            .map(Into::into)
            .collect::<HashSet<_>>();
        let watcher = CoreSession::watch(&stash).await?;

        Ok(Self {
            sessions: existing,
            stash,
            watcher,
        })
    }

    pub async fn next(&mut self) -> Result<Vec<CoreSessionObserverNotification>, StashError> {
        loop {
            self.watcher
                .receiver
                .recv_async()
                .await
                .map_err(|e| StashError::WatcherError(e.to_string()))?;

            let tether = self.stash.connection().await?;
            // Get all sessions
            let current = CoreSession::all(&tether)
                .await?
                .into_iter()
                .map(Into::into)
                .collect::<HashSet<_>>();
            drop(tether);

            // Nothing changed
            if current == self.sessions {
                continue;
            }

            let mut result = Vec::new();

            for session in self.sessions.difference(&current) {
                result.push(CoreSessionObserverNotification::Deleted(
                    session.session_id.clone(),
                    session.user_id.clone(),
                ));
            }

            for session in current.difference(&self.sessions) {
                result.push(CoreSessionObserverNotification::Created(
                    session.session_id.clone(),
                    session.user_id.clone(),
                ));
            }

            self.sessions = current;

            return Ok(result);
        }
    }
}
