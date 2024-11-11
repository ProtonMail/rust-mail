//! Implementation of the [`AuthStore`](proton-api-core::auth::Store) over the database.

use crate::datatypes::{PasswordMode, RemoteId, TfaStatus};
use crate::db::account::{CoreAccount, CoreSession, EncryptedData, SessionEncryptionKey};
use crate::models::ModelExtension;
use crate::os::KeyChain;
use async_trait::async_trait;
use futures::TryFutureExt;
use proton_api_core::auth::{Auth, KeySecret, Tokens, UserKeySecret};
use proton_api_core::store::{Store, StoreError};
use secrecy::{ExposeSecret, SecretString, SecretVec};
use stash::orm::Model;
use stash::stash::Stash;
use std::ops::Deref;
use std::sync::Arc;
use tracing::{error, info};

/// Auth store implementation which records the data in the session database.
pub struct AuthStore {
    stash: Stash,
    key_chain: Arc<dyn KeyChain>,
    user_id: Option<RemoteId>,
    session_id: Option<RemoteId>,
    name_or_addr: Option<String>,
}

impl AuthStore {
    pub fn new(
        stash: &Stash,
        key_chain: Arc<dyn KeyChain>,
        user_id: Option<RemoteId>,
        session_id: Option<RemoteId>,
    ) -> Self {
        Self {
            key_chain,
            user_id,
            session_id,
            stash: stash.clone(),
            name_or_addr: None,
        }
    }

    fn encryption_key(&self) -> Result<SessionEncryptionKey, StoreError> {
        let key = (self.key_chain.get())
            .map_err(|e| format!("failed to load secret from key chain: {e}"))
            .inspect_err(|e| error!(e))?
            .ok_or("keychain has no decryption key")
            .inspect_err(|e| error!(e))?;

        Ok(SessionEncryptionKey::from_base64(&key).ok_or("invalid encryption key")?)
    }

    async fn try_get_auth(&self) -> Result<Auth, StoreError> {
        let key = self.encryption_key()?;
        let tether = self.stash.connection();

        let Some(account) = (if let Some(id) = &self.user_id {
            CoreAccount::find_by_id(id.to_owned(), &tether).await?
        } else {
            None
        }) else {
            return Ok(Auth::None);
        };

        let Some(session) = (if let Some(id) = &self.session_id {
            CoreSession::find_by_id(id.to_owned(), &tether).await?
        } else {
            None
        }) else {
            return Ok(Auth::None);
        };

        let acctok = session.access_token.decrypt_to_string(&key)?;
        let reftok = session.refresh_token.decrypt_to_string(&key)?;
        let scopes = session.auth_scopes.into_inner();

        Ok(Auth::internal(
            account.remote_id.into_inner(),
            session.remote_id.into_inner(),
            Tokens::access(acctok.expose_secret(), reftok.expose_secret(), scopes),
        ))
    }

    async fn try_get_key_secret(&self) -> Result<Option<UserKeySecret>, StoreError> {
        let key = self.encryption_key()?;
        let tether = self.stash.connection();

        let Some(session) = (if let Some(id) = &self.session_id {
            CoreSession::find_by_id(id.to_owned(), &tether).await?
        } else {
            None
        }) else {
            return Ok(None);
        };

        let key_secret = if let Some(secret) = session.key_secret {
            secret.decrypt_to_bytes(&key)?.expose_secret().to_owned()
        } else {
            return Ok(None);
        };

        Ok(Some(UserKeySecret(KeySecret::new(key_secret))))
    }
}

#[async_trait]
impl Store for AuthStore {
    fn get_name_or_addr(&self) -> Option<&String> {
        self.name_or_addr.as_ref()
    }

    fn set_name_or_addr(&mut self, name_or_addr: &str) {
        self.name_or_addr = Some(name_or_addr.to_owned());
    }

    async fn get_auth(&self) -> Auth {
        info!("getting auth from store");

        self.try_get_auth()
            .map_err(|e| format!("failed to get auth: {e}"))
            .inspect_err(|e| error!(e))
            .unwrap_or_else(|_| Auth::None)
            .await
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError> {
        info!("setting auth in store");

        // Get the user and session IDs from the incoming auth session.
        let user_id = RemoteId::from(auth.user_id().ok_or("missing user ID")?);
        let session_id = RemoteId::from(auth.uid().ok_or("missing session ID")?);

        // Get the encryption key.
        let key = self.encryption_key()?;

        // We write twice, so do it in a transaction.
        let mut tether = self.stash.connection();
        let tx = tether.transaction().await?;

        // Load or create the account.
        if (CoreAccount::find_by_id(user_id.clone(), &tx).await?).is_none() {
            info!("creating account for {user_id}");

            let tfa_status = TfaStatus::None;
            let mbp_mode = PasswordMode::One;
            let name_or_addr = self.name_or_addr.take();
            let name_or_addr = name_or_addr.ok_or("missing name or address")?;

            CoreAccount::new(user_id.clone(), name_or_addr, tfa_status, mbp_mode)
                .save(&tx)
                .inspect_err(|e| error!("failed to save account: {e}"))
                .await?;
        }

        // Load or create the session.
        if let Some(session) = CoreSession::find_by_id(session_id.clone(), &tx).await? {
            session.with_auth(auth, &key)?.save(&tx).await?;
        } else {
            info!("creating session for {user_id}");

            CoreSession::new(auth, &key)?
                .save(&tx)
                .inspect_err(|e| error!("failed to save session: {e}"))
                .await?;
        }

        // Set the user ID if it's not already set.
        if let Some(cur_user_id) = &self.user_id {
            assert_eq!(cur_user_id, &user_id);
        } else {
            info!("setting user ID to {user_id}");
            self.user_id = Some(user_id);
        }

        // Set the session ID if it's not already set.
        if let Some(cur_session_id) = &self.session_id {
            assert_eq!(cur_session_id, &session_id);
        } else {
            info!("setting session ID to {session_id}");
            self.session_id = Some(session_id);
        }

        tx.commit().await?;

        Ok(())
    }

    async fn get_key_secret(&self) -> Option<UserKeySecret> {
        info!("getting key secret from store");

        self.try_get_key_secret()
            .map_err(|e| format!("failed to get key secret: {e}"))
            .inspect_err(|e| error!(e))
            .unwrap_or_else(|_| None)
            .await
    }

    async fn set_key_secret(&mut self, sec: UserKeySecret) -> Result<(), StoreError> {
        info!("setting key secret in store");

        // Get the encryption key.
        let key = self.encryption_key()?;

        // We write twice, so do it in a transaction.
        let mut tether = self.stash.connection();
        let tx = tether.transaction().await?;

        let Some(user_id) = self.user_id.clone() else {
            return Err("failed to set user secrets: no user ID")?;
        };

        let Some(account) = CoreAccount::find_by_id(user_id.clone(), &tx).await? else {
            return Err(format!("failed to set user secrets: missing {user_id}"))?;
        };

        for session in CoreSession::find_by_user_id(user_id, &tx, None).await? {
            session.with_key_secret(&sec, &key)?.save(&tx).await?;
        }

        if !account.is_ready {
            account.with_ready().save(&tx).await?;
        }

        tx.commit().await?;

        Ok(())
    }

    async fn clear(&mut self) -> Result<(), StoreError> {
        let mut tether = self.stash.connection();

        // Clear the session if it exists.
        if let Some(id) = &self.session_id {
            let tx = tether.transaction().await?;
            CoreSession::delete_by_remote_id(id.to_owned(), &tx).await?;
            tx.commit().await?;
        }

        // Clear the user and session IDs.
        self.user_id = None;
        self.session_id = None;

        Ok(())
    }
}

pub(crate) trait DecryptExt
where
    for<'a> &'a Self: Deref<Target = EncryptedData>,
{
    fn decrypt_to_bytes(&self, key: &SessionEncryptionKey) -> Result<SecretVec<u8>, StoreError> {
        Ok(key.decrypt(self)?.into())
    }

    fn decrypt_to_string(&self, key: &SessionEncryptionKey) -> Result<SecretString, StoreError> {
        Ok(String::from_utf8(key.decrypt(self)?)?.into())
    }
}

impl<This> DecryptExt for This where for<'a> &'a This: Deref<Target = EncryptedData> {}
