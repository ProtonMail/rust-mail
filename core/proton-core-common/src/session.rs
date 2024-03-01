use crate::keychain::{session_encryption_key_from_key_chain, SessionKeyChain};
use proton_api_core::auth::{Auth, AuthScope};
use proton_api_core::domain::{ExposeSecret, SecretString, Uid};
use proton_api_core::exports::tracing::error;
use proton_api_core::exports::{thiserror, tracing};
use proton_core_db::proton_sqlite3::SqliteConnectionPool;
use proton_core_db::{
    DBResult, DecryptedUserSession, EncryptedData, SessionEncryptionKey, SessionSqliteConnection,
};
use std::error::Error;

/// Receive notifications when the session has been refreshed or deleted.
pub trait CoreSessionCallback: Send + Sync {
    fn on_session_refresh(&self);
    fn on_session_deleted(&self);

    fn on_error(&self, err: &CoreSessionError);
}

#[derive(Debug, thiserror::Error)]
pub enum CoreSessionError {
    #[error("Database Error: {0}")]
    DB(#[from] proton_core_db::DBError),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(Box<dyn Error + Send>),
}

/// Core session retrieves the session
pub(crate) struct CoreSession {
    auth: Option<Auth>,
    db: SqliteConnectionPool,
    cb: Option<Box<dyn CoreSessionCallback>>,
    keychain: Box<dyn SessionKeyChain>,
}

impl CoreSession {
    pub(crate) fn new(
        session: Option<DecryptedUserSession>,
        pool: SqliteConnectionPool,
        keychain: Box<dyn SessionKeyChain>,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> Self {
        Self {
            auth: session.map(decrypted_session_to_auth),
            db: pool,
            cb,
            keychain,
        }
    }

    fn new_connection(&self) -> Result<SessionSqliteConnection, CoreSessionError> {
        let conn = self.db.acquire()?;
        Ok(conn.into())
    }

    fn get_encryption_key(&self) -> Result<SessionEncryptionKey, CoreSessionError> {
        let bytes = self
            .keychain
            .get_or_error()
            .map_err(CoreSessionError::KeyChain)?;
        session_encryption_key_from_key_chain(bytes)
    }

    fn encrypt_tokens(
        &self,
        key: SessionEncryptionKey,
        access: &SecretString,
        refresh: &SecretString,
    ) -> Result<(EncryptedData, EncryptedData), CoreSessionError> {
        let access = key
            .encrypt(access.expose_secret().as_bytes())
            .map_err(|_| CoreSessionError::Crypto)?;
        let refresh = key
            .encrypt(refresh.expose_secret().as_bytes())
            .map_err(|_| CoreSessionError::Crypto)?;
        Ok((access, refresh))
    }
    fn on_error(&self, error: &CoreSessionError) {
        if let Some(cb) = &self.cb {
            cb.on_error(error);
        }
    }
}

impl proton_api_core::auth::AuthStore for CoreSession {
    fn get_auth(&self) -> Option<&Auth> {
        self.auth.as_ref()
    }

    #[tracing::instrument(skip(self), fields(uid=?uid))]
    fn set_auth(
        &mut self,
        uid: Uid,
        refresh_token: SecretString,
        access_token: SecretString,
        scopes: AuthScope,
    ) -> Result<(), Box<dyn Error>> {
        let session_key = self.get_encryption_key().map_err(|e| {
            error!("Failed to retrieve encryption key from keychain: {e}");
            self.on_error(&e);
            e
        })?;

        let (encrypted_access_token, encrypted_refresh_token) = self
            .encrypt_tokens(session_key, &access_token, &refresh_token)
            .map_err(|e| {
                error!("Failed to encrypt tokens");
                self.on_error(&e);
                Box::new(e)
            })?;

        let mut conn = self.new_connection().map_err(|e| {
            error!("Failed to get database connection:{e}");
            self.on_error(&e);
            e
        })?;

        {
            let scopes = &scopes;
            let uid_ref = &uid;
            conn.tx(|tx| -> DBResult<()> {
                tx.update_session(
                    uid_ref,
                    &encrypted_access_token,
                    &encrypted_refresh_token,
                    Some(scopes),
                )
            })
            .map_err(|e| {
                let e = CoreSessionError::DB(e);
                error!("Failed write auth to database:{e}");
                self.on_error(&e);
                e
            })?;
        }

        self.auth = Some(Auth {
            uid,
            refresh_token,
            access_token,
            scope: scopes,
        });

        Ok(())
    }

    fn set_scopes(&mut self, _: AuthScope) -> Result<Option<&Auth>, Box<dyn Error>> {
        todo!()
    }

    fn clear_auth(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(auth) = self.auth.take() else {
            return Ok(());
        };

        tracing::debug_span!("clear_auth", uid=?auth.uid).in_scope(
            || -> Result<(), Box<dyn Error>> {
                let mut conn = self.new_connection().map_err(|e| {
                    error!("Failed to get db connection: {e}");
                    e
                })?;

                conn.tx(|tx| -> DBResult<()> { tx.delete_session(&auth.uid) })
                    .map_err(|e| {
                        let e = CoreSessionError::DB(e);
                        error!("Failed to remove session from db: {e}");
                        e
                    })?;

                Ok(())
            },
        )
    }
}

fn decrypted_session_to_auth(session: DecryptedUserSession) -> Auth {
    Auth {
        uid: session.session_id,
        refresh_token: session.refresh_token,
        access_token: session.access_token,
        scope: session.scopes.unwrap_or(AuthScope::from("")),
    }
}
