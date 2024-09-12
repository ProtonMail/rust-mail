//! Implementation of the [`AuthStore`](proton-api-core::auth::Store) over the database.

use crate::datatypes::{PasswordMode, RemoteId, TfaStatus};
use crate::db::account::{CoreAccount, CoreSession, EncryptedData, SessionEncryptionKey};
use crate::models::ModelExtension;
use crate::os::KeyChain;
use futures::future::BoxFuture;
use futures::FutureExt;
use proton_api_core::auth::{AuthSession, SecretString, Store, StoreError, UserSecrets};
use secrecy::{ExposeSecret, SecretVec};
use stash::orm::Model;
use stash::stash::{Interface, Stash};
use std::ops::Deref;
use std::sync::Arc;
use tracing::error;

/// Auth store implementation which records the data in the session database.
pub struct AuthStore {
    stash: Stash,
    key_chain: Arc<dyn KeyChain>,
    user_id: Option<RemoteId>,
    session_id: Option<RemoteId>,
}

impl AuthStore {
    pub fn new(
        stash: Stash,
        key_chain: Arc<dyn KeyChain>,
        user_id: Option<RemoteId>,
        session_id: Option<RemoteId>,
    ) -> Self {
        Self {
            stash,
            key_chain,
            user_id,
            session_id,
        }
    }

    fn encryption_key(&self) -> Result<SessionEncryptionKey, StoreError> {
        let Some(key) = self.key_chain.get().map_err(|e| -> StoreError {
            error!("Failed to load secret from key chain: {e}");
            format!("Failed to load secret from key chain: {e}").into()
        })?
        else {
            error!("Keychain has no decryption key");
            return Err("Keychain has no decryption key".into());
        };

        SessionEncryptionKey::from_base64(&key).ok_or("Invalid encryption key".into())
    }

    async fn get_session(&self) -> Result<Option<AuthSession>, StoreError> {
        let key = self.encryption_key()?;

        let Some(account) = (if let Some(id) = &self.user_id {
            CoreAccount::find_by_id(id.to_owned(), &self.stash).await?
        } else {
            None
        }) else {
            return Ok(None);
        };

        let Some(session) = (if let Some(id) = &self.session_id {
            CoreSession::find_by_id(id.to_owned(), &self.stash).await?
        } else {
            None
        }) else {
            return Ok(None);
        };

        Ok(Some(AuthSession {
            uid: session.remote_id.into(),
            name_or_addr: account.name_or_addr,
            user_id: session.account_id.into(),
            second_factor_mode: account.second_factor_mode.into(),
            password_mode: account.password_mode.into(),
            access_token: session.access_token.decrypt_to_string(&key)?,
            refresh_token: session.refresh_token.decrypt_to_string(&key)?,
            auth_scope: session.auth_scope.into_inner(),
            auth_state: session.auth_state.into(),
        }))
    }

    async fn get_secrets(&self) -> Result<Option<UserSecrets>, StoreError> {
        let key = self.encryption_key()?;

        let Some(session) = (if let Some(id) = &self.session_id {
            CoreSession::find_by_id(id.to_owned(), &self.stash).await?
        } else {
            None
        }) else {
            return Ok(None);
        };

        let Some(key_secret) = session.key_secret else {
            return Ok(None);
        };

        Ok(Some(UserSecrets {
            key_secret: key_secret
                .decrypt_to_bytes(&key)?
                .expose_secret()
                .to_owned()
                .into(),
        }))
    }

    async fn set_session(&mut self, auth: AuthSession) -> Result<(), StoreError> {
        let key = self.encryption_key()?;

        // Get the user and session IDs from the incoming auth session.
        let user_id = RemoteId::from(auth.user_id.clone());
        let session_id = RemoteId::from(auth.uid.clone());
        let tfa_mode = TfaStatus::from(auth.second_factor_mode);
        let mbp_mode = PasswordMode::from(auth.password_mode);

        // We write twice, so do it in a transaction.
        let tx = self.stash.transaction().await?;

        // Attempt to load the account.
        let account = if let Some(id) = &self.user_id {
            CoreAccount::find_by_id(id.to_owned(), &tx).await?
        } else {
            None
        };

        // If the account doesn't exist, create it.
        if let Some(account) = account {
            assert_eq!(account.remote_id, user_id);
        } else {
            let user_id = user_id.clone();
            let name_or_addr = auth.name_or_addr.clone();

            (CoreAccount::new(user_id, name_or_addr, tfa_mode, mbp_mode).save_using(&tx)).await?;
        }

        // Attempt to load the session.
        let session = if let Some(id) = &self.session_id {
            CoreSession::find_by_id(id.to_owned(), &tx).await?
        } else {
            None
        };

        // Update the session or create a new one.
        if let Some(session) = session {
            session.with_auth(&auth, &key)?.save_using(&tx).await?;
        } else {
            CoreSession::new(auth, &key)?.save_using(&tx).await?;
        }

        // Set the user ID if it's not already set.
        if let Some(cur_user_id) = &self.user_id {
            assert_eq!(cur_user_id, &user_id);
        } else {
            self.user_id = Some(user_id);
        }

        // Set the session ID if it's not already set.
        if let Some(cur_session_id) = &self.session_id {
            assert_eq!(cur_session_id, &session_id);
        } else {
            self.session_id = Some(session_id);
        }

        // Commit the transaction.
        Ok(tx.commit().await?)
    }

    async fn set_secrets(&mut self, secrets: UserSecrets) -> Result<(), StoreError> {
        let Some(session) = (if let Some(id) = &self.session_id {
            CoreSession::find_by_id(id.to_owned(), &self.stash).await?
        } else {
            None
        }) else {
            return Err("session must exist to set secrets")?;
        };

        session
            .with_key_secret(&secrets.key_secret, &self.encryption_key()?)?
            .save_using(&self.stash)
            .await?;

        Ok(())
    }

    async fn clear(&mut self) -> Result<(), StoreError> {
        // Clear the session if it exists.
        if let Some(id) = &self.session_id {
            CoreSession::delete_by_remote_id(id.to_owned(), &self.stash).await?;
        }

        // Clear the user and session IDs.
        self.user_id = None;
        self.session_id = None;

        Ok(())
    }
}

impl Store for AuthStore {
    fn get_session(&self) -> BoxFuture<Result<Option<AuthSession>, StoreError>> {
        self.get_session().boxed()
    }

    fn get_secrets(&self) -> BoxFuture<Result<Option<UserSecrets>, StoreError>> {
        self.get_secrets().boxed()
    }

    fn set_session(&mut self, auth: AuthSession) -> BoxFuture<Result<(), StoreError>> {
        self.set_session(auth).boxed()
    }

    fn set_secrets(&mut self, secrets: UserSecrets) -> BoxFuture<Result<(), StoreError>> {
        self.set_secrets(secrets).boxed()
    }

    fn clear(&mut self) -> BoxFuture<Result<(), StoreError>> {
        self.clear().boxed()
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
