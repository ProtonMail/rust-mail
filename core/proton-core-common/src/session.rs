use crate::db::{
    DecryptedUserSession, EncryptedAccessToken, EncryptedRefreshToken, EncryptedUserSession,
    SessionEncryptionKey,
};
use crate::os::{KeyChain, KeyChainError};
use futures::executor::block_on;
use proton_api_core::auth::{AccessToken, Auth, RefreshToken, Scope};
use proton_api_core::domain::{ExposeSecret, SecretString, Uid};
use proton_api_core::exports::anyhow::anyhow;
use proton_api_core::exports::tracing::{debug, error};
use proton_api_core::exports::{anyhow, thiserror, tracing};
use proton_api_core::http::RequestError;
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;
use std::error::Error;
use std::sync::Arc;

/// Receive notifications when the session has been refreshed or deleted.
pub trait CoreSessionCallback: Send + Sync {
    /// Triggered when the session has been refreshed.
    fn on_session_refresh(&self);
    /// Triggered when the session has been destroyed.
    fn on_session_deleted(&self);

    /// Triggered when the refresh operation fails.
    fn on_refresh_failed(&self, e: &RequestError);

    /// Triggers if any error occurs while persisting the session data.
    fn on_error(&self, err: &CoreSessionError);
}

#[derive(Debug, thiserror::Error)]
pub enum CoreSessionError {
    #[error("Database Error: {0}")]
    DB(#[from] crate::db::DBError),
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(#[from] KeyChainError),
    #[error("Keychain has no encryption key")]
    KeyChainHasNoKey,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

/// Core session retrieves the session
pub(crate) struct CoreSession {
    auth: Option<Auth>,
    stash: Stash,
    cb: Option<Box<dyn CoreSessionCallback>>,
    keychain: Arc<dyn KeyChain>,
}

impl CoreSession {
    pub(crate) fn new(
        session: Option<DecryptedUserSession>,
        stash: Stash,
        keychain: Arc<dyn KeyChain>,
        cb: Option<Box<dyn CoreSessionCallback>>,
    ) -> Self {
        Self {
            auth: session.map(decrypted_session_to_auth),
            stash,
            cb,
            keychain,
        }
    }

    fn get_encryption_key(&self) -> Result<SessionEncryptionKey, CoreSessionError> {
        let string = SecretString::new(
            self.keychain
                .get()?
                .ok_or(CoreSessionError::KeyChainHasNoKey)?,
        );
        SessionEncryptionKey::from_base64(string.expose_secret()).ok_or(CoreSessionError::Crypto)
    }

    fn encrypt_tokens(
        key: &SessionEncryptionKey,
        access: &AccessToken,
        refresh: &RefreshToken,
    ) -> Result<(EncryptedAccessToken, EncryptedRefreshToken), CoreSessionError> {
        let access =
            EncryptedAccessToken::new(access, key).map_err(|_| CoreSessionError::Crypto)?;
        let refresh =
            EncryptedRefreshToken::new(refresh, key).map_err(|_| CoreSessionError::Crypto)?;
        Ok((access, refresh))
    }
    fn on_error(&self, error: &CoreSessionError) {
        if let Some(cb) = &self.cb {
            cb.on_error(error);
        }
    }
}

impl proton_api_core::auth::Store for CoreSession {
    fn get_auth(&self) -> Option<&Auth> {
        self.auth.as_ref()
    }

    #[tracing::instrument(skip(self,auth), fields(uid = ?auth.uid, user_id= ?auth.user_id))]
    fn set_auth(&mut self, auth: Auth) -> Result<(), Box<dyn Error>> {
        let session_key = self.get_encryption_key().map_err(|e| {
            error!("Failed to retrieve encryption key from keychain: {e}");
            self.on_error(&e);
            e
        })?;

        let (encrypted_access_token, encrypted_refresh_token) =
            Self::encrypt_tokens(&session_key, &auth.access_token, &auth.refresh_token).map_err(
                |e| {
                    error!("Failed to encrypt tokens");
                    self.on_error(&e);
                    Box::new(e)
                },
            )?;

        let mut encrypted_session = auth_to_encrypted_session(
            auth.clone(),
            encrypted_access_token,
            encrypted_refresh_token,
        );
        block_on(async { encrypted_session.save().await })?;
        self.auth = Some(auth);
        Ok(())
    }

    fn refresh_auth_failed(&self, e: &RequestError) {
        if let Some(cb) = &self.cb {
            cb.on_refresh_failed(e);
        }
    }

    #[tracing::instrument(skip(self), fields(uid = ? uid))]
    fn refresh_auth(
        &mut self,
        uid: Uid,
        access_token: AccessToken,
        refresh_token: RefreshToken,
        scope: Scope,
    ) -> Result<(), Box<dyn Error>> {
        let user_id = {
            let Some(auth) = &self.auth else {
                return Err(Box::new(CoreSessionError::Other(anyhow!(
                    "no auth into to refresh"
                ))));
            };
            auth.user_id.clone()
        };
        let session_key = self.get_encryption_key().map_err(|e| {
            error!("Failed to retrieve encryption key from keychain: {e}");
            self.on_error(&e);
            e
        })?;

        let (encrypted_access_token, encrypted_refresh_token) =
            Self::encrypt_tokens(&session_key, &access_token, &refresh_token).map_err(|e| {
                error!("Failed to encrypt tokens");
                self.on_error(&e);
                Box::new(e)
            })?;

        block_on(async {
            let mut session = EncryptedUserSession::load(user_id.clone(), &self.stash)
                .await?
                .unwrap();
            session.user_id = user_id;
            session.access_token = encrypted_access_token;
            session.refresh_token = encrypted_refresh_token;
            session.scopes = scope.clone();
            session.save().await
        })?;

        if let Some(cur_auth) = &mut self.auth {
            cur_auth.uid = uid;
            cur_auth.access_token = access_token;
            cur_auth.refresh_token = refresh_token;
            cur_auth.scope = scope;
        }

        Ok(())
    }

    fn set_scopes(&mut self, _: Scope) -> Result<Option<&Auth>, Box<dyn Error>> {
        todo!()
    }

    fn clear_auth(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(auth) = self.auth.take() else {
            return Ok(());
        };

        tracing::debug_span!("clear_auth", uid=?auth.uid, ?auth.user_id).in_scope(
            || -> Result<(), Box<dyn Error>> {
                debug!("Deleting session");
                block_on(async {
                    self.stash
                        .execute(
                            "DELETE FROM core_sessions WHERE user_id =?",
                            params![auth.user_id],
                        )
                        .await
                })?;
                Ok(())
            },
        )
    }
}

fn decrypted_session_to_auth(session: DecryptedUserSession) -> Auth {
    Auth {
        email: session.email,
        user_id: session.user_id,
        uid: session.session_id,
        refresh_token: session.refresh_token,
        access_token: session.access_token,
        scope: session.scopes,
    }
}

fn auth_to_encrypted_session(
    auth: Auth,
    encrypted_access_token: EncryptedAccessToken,
    encrypted_refresh_token: EncryptedRefreshToken,
) -> EncryptedUserSession {
    EncryptedUserSession {
        session_id: auth.uid,
        user_id: auth.user_id,
        name: None,
        email: auth.email,
        refresh_token: encrypted_refresh_token,
        access_token: encrypted_access_token,
        scopes: auth.scope,
        row_id: None,
        stash: None,
    }
}
