#[cfg(test)]
#[path = "../../tests/db/types.rs"]
mod tests;

use crate::datatypes::{
    AccountDetails, AuthScopes, AvatarInformation, PasswordMode, TfaStatus, Timestamp,
};
use crate::models::ModelExtension;
use aes_gcm::aead::consts::U12;
use aes_gcm::aead::Nonce;
use aes_gcm::aes::Aes256;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, AesGcm, Key, KeySizeUser,
};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use proton_api_core::auth::{Tokens, UserKeySecret};
use proton_api_core::services::proton::common::{AuthId, UserId};
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use sqlite_watcher::watcher::TableObserver;
use stash::exports::SqliteError;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use stash::{params, sql_using_serde};
use std::collections::BTreeSet;
use std::ops::Deref;
use std::string::FromUtf8Error;
use thiserror::Error;
use zeroize::Zeroize;

#[derive(Debug, Clone, PartialEq, Eq, Model)]
#[TableName("core_accounts")]
pub struct CoreAccount {
    /// Remote ID of the account (i.e. the API User ID).
    #[IdField]
    pub remote_id: UserId,

    /// The account's username or email address (used for login).
    #[DbField]
    pub name_or_addr: String,

    /// The second factor auth mode of the account.
    #[DbField]
    pub second_factor_mode: Option<TfaStatus>,

    /// The mailbox password mode of the account.
    #[DbField]
    pub password_mode: Option<PasswordMode>,

    /// The account's username (once known).
    #[DbField]
    pub username: Option<String>,

    /// The account's display name (once known).
    #[DbField]
    pub display_name: Option<String>,

    /// The account's primary email address (once known).
    #[DbField]
    pub primary_addr: Option<String>,

    /// Timestamp of when the account was last set as the primary account.
    #[DbField]
    pub primary_at: Option<Timestamp>,

    /// Whether the account is ready (i.e. login flow completed).
    #[DbField]
    pub is_ready: bool,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl CoreAccount {
    /// Create a new account.
    #[must_use]
    pub fn new(remote_id: UserId, name_or_addr: String) -> Self {
        Self {
            remote_id,
            name_or_addr,
            is_ready: false,

            // --- Optional fields ---
            username: None,
            display_name: None,
            primary_addr: None,
            second_factor_mode: None,
            password_mode: None,
            primary_at: None,
            row_id: None,
        }
    }

    /// List all accounts, ordered by the primary timestamp.
    ///
    /// # Errors
    ///
    /// Returns error if the retrieval fails.
    pub async fn by_primary_at(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find("ORDER BY primary_at DESC", vec![], tether).await
    }

    /// Update the username of the account.
    #[must_use]
    pub fn with_username(self, username: String) -> Self {
        Self {
            username: Some(username),

            // --- preserve ---
            ..self
        }
    }

    /// Update the display name of the account.
    #[must_use]
    pub fn with_display_name(self, display_name: String) -> Self {
        Self {
            display_name: Some(display_name),

            // --- preserve ---
            ..self
        }
    }

    /// Update the primary email address of the account.
    #[must_use]
    pub fn with_primary_addr(self, primary_addr: String) -> Self {
        Self {
            primary_addr: Some(primary_addr),

            // --- preserve ---
            ..self
        }
    }

    /// Update the 2FA mode of the account.
    #[must_use]
    pub fn with_tfa_mode(self, mode: TfaStatus) -> Self {
        Self {
            second_factor_mode: Some(mode),

            // --- preserve ---
            ..self
        }
    }

    /// Update the mailbox password mode of the account.
    #[must_use]
    pub fn with_mbp_mode(self, mode: PasswordMode) -> Self {
        Self {
            password_mode: Some(mode),

            // --- preserve ---
            ..self
        }
    }

    /// Update the primary timestamp to now.
    #[must_use]
    pub fn with_primary_now(self) -> Self {
        Self {
            primary_at: Some(Timestamp::now()),

            // --- preserve ---
            ..self
        }
    }

    /// Mark the account as ready.
    #[must_use]
    pub fn with_ready(self) -> Self {
        Self {
            is_ready: true,

            // --- preserve ---
            ..self
        }
    }

    /// Save a account to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing accounts are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(existing) = Self::find_by_id(self.remote_id.clone(), bond).await? {
            self.row_id = existing.row_id;
        }

        <Self as Model>::save(self, bond).await
    }

    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(CoreAccountWatcher { sender }))
    }

    /// Retrieves account details including the name, email, and avatar information.
    ///
    /// This method constructs the account details using the available fields. If the display name
    /// or username is not set, it falls back to `name_or_addr`. Similarly, the email defaults to
    /// `name_or_addr` if the primary address is unavailable.
    ///
    /// # Returns
    /// - `AccountDetails`: A struct containing the account's name, email, and avatar information.
    #[must_use]
    pub fn details(&self) -> AccountDetails {
        let name = self
            .display_name
            .clone()
            .or(self.username.clone())
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
                tracing::error!("Failed to send notification for CoreAccountWatcher: {}", e);
            })
            .ok();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Model)]
#[TableName("core_sessions")]
pub struct CoreSession {
    /// Remote ID of the session (i.e. the API Auth UID).
    #[IdField]
    pub remote_id: AuthId,

    /// Account ID the session is associated with (i.e. the API User ID).
    #[DbField]
    pub account_id: UserId,

    /// Access token for the session.
    #[DbField]
    pub access_token: EncryptedAccessToken,

    /// Refresh token for the session.
    #[DbField]
    pub refresh_token: EncryptedRefreshToken,

    /// The scope(s) the session has access to.
    #[DbField]
    pub auth_scopes: AuthScopes,

    /// Secret used for unlocking the account's PGP key (once derived).
    #[DbField]
    pub key_secret: Option<EncryptedKeySecret>,

    #[RowIdField]
    pub row_id: Option<u64>,
}

#[derive(Debug, Error)]
pub enum CoreSessionError {
    #[error("missing auth UID")]
    AuthUid,

    #[error("missing auth user ID")]
    AuthUserId,

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
        session_id: AuthId,
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
            row_id: None,
        })
    }

    /// Update the auth tokens.
    ///
    /// # Errors
    ///
    /// Returns an error if the encryption fails.
    ///
    /// # Panics
    ///
    /// Panics if the UID in the auth does not match the session's remote ID.
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

    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(CoreSessionWatcher { sender }))
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
                tracing::error!("Failed to send notification for CoreSessionWatcher: {}", e);
            })
            .ok();
    }
}

#[derive(Debug, Error)]
pub enum DecryptionError {
    #[error("Decryption failed")]
    Decryption,

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
    fn to_sql(&self) -> Result<ToSqlOutput, SqliteError> {
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
    fn to_sql(&self) -> Result<ToSqlOutput, SqliteError> {
        self.0.to_sql()
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
        let Ok(bytes) = BASE64_STANDARD.decode(value) else {
            return None;
        };

        let Ok(key) = Self::with_bytes(bytes) else {
            return None;
        };

        Some(key)
    }
}
