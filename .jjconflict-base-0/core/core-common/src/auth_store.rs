//! Implementation of the [`AuthStore`](proton-core-api::auth::Store) over the database.

use crate::db::account::{CoreAccount, CoreSession, EncryptedData, SessionEncryptionKey};
use crate::models::ModelExtension;
use crate::os::{KeyChain, KeyChainExt};
use anyhow::{Context, bail};
use async_trait::async_trait;
use futures::TryFutureExt;
use proton_core_api::auth::{Auth, Tokens, UserKeySecret};
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::store::{AuthInfo, Store, StoreError, UserData};
use secrecy::{ExposeSecret, SecretString, SecretVec};
use stash::AccountDb;
use stash::orm::Model;
use stash::stash::Stash;
use std::ops::Deref;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Auth store implementation which records the data in the session database.
pub struct AuthStore {
    stash: Stash<AccountDb>,
    key_chain: Arc<dyn KeyChain>,
    user_id: Option<UserId>,
    session_id: Option<SessionId>,
    name_or_addr: Option<String>,
}

impl AuthStore {
    pub fn new(
        stash: Stash<AccountDb>,
        key_chain: Arc<dyn KeyChain>,
        user_id: Option<UserId>,
        session_id: Option<SessionId>,
    ) -> Self {
        Self {
            key_chain,
            user_id,
            session_id,
            stash,
            name_or_addr: None,
        }
    }

    fn encryption_key(&self) -> Result<SessionEncryptionKey, StoreError> {
        let key = (self.key_chain.load::<SessionEncryptionKey>())
            .context("failed to load secret from key chain")
            .inspect_err(|e| error!("{e:?}"))?
            .context("keychain has no decryption key")
            .inspect_err(|e| error!("{e:?}"))?;

        Ok(key)
    }

    async fn try_get_auth(&self) -> Result<Auth, StoreError> {
        let key = self.encryption_key()?;
        let tether = self.stash.connection().await?;

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

    async fn try_expose_key_secret(&self) -> Result<Option<UserKeySecret>, StoreError> {
        let key = self.encryption_key()?;
        let tether = self.stash.connection().await?;

        let Some(session_id) = self.session_id.clone() else {
            return Ok(None);
        };

        let Some(session) = CoreSession::find_by_id(session_id.clone(), &tether).await? else {
            return Ok(None);
        };

        let secret = if let Some(secret) = session.key_secret {
            secret.decrypt_to_bytes(&key)?.expose_secret().to_owned()
        } else {
            return Ok(None);
        };

        Ok(Some(UserKeySecret::from(secret)))
    }
}

#[async_trait]
impl Store for AuthStore {
    fn set_name_or_addr(&mut self, name_or_addr: &str) {
        self.name_or_addr = Some(name_or_addr.to_owned());
    }

    async fn get_auth(&self) -> Auth {
        self.try_get_auth().await.unwrap_or_else(|e| {
            error!("failed to get auth: {e:?}");
            Auth::None
        })
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<(), StoreError> {
        match auth {
            Auth::None => {
                return self.clear_session().await;
            }

            Auth::External { .. } => {
                warn!("ignoring external auth");
                return Ok(());
            }

            Auth::Anonymous { .. } => {
                info!("ignoring anonymous auth");
                return Ok(());
            }

            Auth::Internal { .. } => {
                info!("setting auth in store");
            }
        }

        // Get the user and session IDs from the incoming auth session.
        let user_id = UserId::from(auth.user_id().context("missing user ID")?);
        let session_id = SessionId::from(auth.uid().context("missing session ID")?);
        let tokens = auth.tokens().context("missing tokens")?;

        // Get the encryption key.
        let key = self.encryption_key()?;

        // We write twice, so do it in a transaction.
        self.stash
            .connection()
            .await?
            .tx(async |tx| {
                // Load or create the account.
                if (CoreAccount::find_by_id(user_id.clone(), tx).await?).is_none() {
                    info!("creating account for {user_id}");

                    let name_or_addr = self.name_or_addr.take();
                    // Ensures a non-null value for the name_or_addr field to satisfy database update requirements.
                    // A default empty string is used when the value is None, as certain mobile callbacks expect this field to be non-null.
                    let name_or_addr = name_or_addr.unwrap_or_else(|| String::from("Unknown"));

                    CoreAccount::new(user_id.clone(), name_or_addr)
                        .save(tx)
                        .inspect_err(|e| error!("failed to save account: {e:?}"))
                        .await?;
                }

                // Load or create the session.
                if let Some(session) = CoreSession::find_by_id(session_id.clone(), tx).await? {
                    session.with_tokens(tokens, &key)?.save(tx).await?;
                } else {
                    info!("creating session for {user_id}");

                    CoreSession::new(user_id.clone(), session_id.clone(), tokens, &key)?
                        .save(tx)
                        .inspect_err(|e| error!("failed to save session: {e:?}"))
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

                Ok(())
            })
            .await
    }

    async fn set_auth_info(&mut self, info: AuthInfo) -> Result<(), StoreError> {
        info!("setting auth info in store");

        // Get the user and session IDs from the incoming auth info.
        let user_id = info.user_id;
        let session_id = info.session_id;
        let tfa_mode = info.tfa_mode.into();

        // We write twice, so do it in a transaction.
        self.stash
            .connection()
            .await?
            .tx(async |tx| {
                // Load or create the account.
                if let Some(account) = CoreAccount::find_by_id(user_id.clone(), tx).await? {
                    info!("updating account info for {user_id}");

                    account.with_tfa_mode(tfa_mode).save(tx).await?;
                } else {
                    info!("creating account for {user_id}");

                    let name_or_addr = self.name_or_addr.take();
                    let name_or_addr = name_or_addr.context("missing name or address")?;

                    CoreAccount::new(user_id.clone(), name_or_addr)
                        .with_tfa_mode(tfa_mode)
                        .save(tx)
                        .inspect_err(|e| error!("failed to save account: {e:?}"))
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
                Ok(())
            })
            .await
    }

    async fn set_pass(&mut self, pass: &str) -> Result<(), StoreError> {
        info!("setting pass in store");

        self.stash
            .connection()
            .await?
            .tx(async |tx| {
                let key = self.encryption_key()?;

                let Some(user_id) = self.user_id.clone() else {
                    bail!("failed to set pass: no user ID");
                };

                let Some(account) = CoreAccount::find_by_id(user_id.clone(), tx).await? else {
                    bail!("failed to set pass: missing {user_id}");
                };

                account.with_password(pass, &key)?.save(tx).await?;

                Ok(())
            })
            .await
    }

    async fn clear_pass(&mut self) -> Result<(), StoreError> {
        info!("clearing pass in store");

        self.stash
            .connection()
            .await?
            .tx(async |tx| {
                let Some(user_id) = self.user_id.clone() else {
                    bail!("failed to set pass: no user ID");
                };

                let Some(account) = CoreAccount::find_by_id(user_id.clone(), tx).await? else {
                    bail!("failed to set pass: missing {user_id}");
                };

                account.without_password().save(tx).await?;

                Ok(())
            })
            .await
    }

    async fn set_temp_pass(&mut self, value: bool) -> Result<(), StoreError> {
        info!("setting temp pass in store");

        self.stash
            .connection()
            .await?
            .tx(async |tx| {
                let Some(user_id) = self.user_id.clone() else {
                    bail!("failed to set temp pass: no user ID");
                };

                let Some(account) = CoreAccount::find_by_id(user_id.clone(), tx).await? else {
                    bail!("failed to set temp pass: missing {user_id}");
                };

                account.with_temp_pass(value).save(tx).await?;

                Ok(())
            })
            .await
    }

    async fn set_user_data(&mut self, data: UserData) -> Result<(), StoreError> {
        info!("setting user data in store");

        let key = self.encryption_key()?;

        self.stash
            .connection()
            .await?
            .tx(async |tx| {
                let Some(user_id) = self.user_id.clone() else {
                    bail!("failed to set user data: no user ID");
                };

                let Some(account) = CoreAccount::find_by_id(user_id.clone(), tx).await? else {
                    bail!("failed to set user data: missing {user_id}");
                };

                for session in CoreSession::find_by_user_id(user_id.clone(), tx).await? {
                    session
                        .with_key_secret(&data.key_secret, &key)?
                        .save(tx)
                        .await?;
                }

                account
                    .with_username(data.username.clone())
                    .with_name_or_addr(data.username)
                    .with_display_name(data.display_name)
                    .with_primary_addr(data.primary_addr)
                    .with_mbp_mode(data.password_mode.into())
                    .with_ready()
                    .save(tx)
                    .await?;

                Ok(())
            })
            .await
    }

    async fn set_key_secret(&mut self, secret: UserKeySecret) -> Result<(), StoreError> {
        info!("setting key secret in store");

        // Get the encryption key and its secret.
        let key = self.encryption_key()?;

        // We write twice, so do it in a transaction.
        let mut tether = self.stash.connection().await?;
        tether
            .tx(async |tx| {
                let Some(user_id) = self.user_id.clone() else {
                    bail!("failed to set user data: no user ID");
                };

                for session in CoreSession::find_by_user_id(user_id, tx).await? {
                    session.with_key_secret(&secret, &key)?.save(tx).await?;
                }

                Ok(())
            })
            .await
    }

    async fn expose_key_secret(&self) -> Option<UserKeySecret> {
        info!("exposing key secret from store");

        self.try_expose_key_secret()
            .map_err(|e| format!("failed to expose key secret: {e}"))
            .inspect_err(|e| error!(e))
            .unwrap_or_else(|_| None)
            .await
    }

    async fn clear_session(&mut self) -> Result<(), StoreError> {
        info!("clearing session from store");

        // Clear the session if it exists.
        if let Some(id) = &self.session_id {
            self.stash
                .connection()
                .await?
                .tx(async |tx| CoreSession::delete_by_id(id.to_owned(), tx).await)
                .await?;
        }

        // Clear the user and session IDs.
        self.session_id = None;

        Ok(())
    }

    async fn clear_account(&mut self) -> Result<(), StoreError> {
        info!("clearing account from store");

        // First, clear the session.
        self.clear_session().await?;

        // Clear the account if it exists.
        if let Some(id) = &self.user_id {
            self.stash
                .connection()
                .await?
                .tx(async |tx| CoreAccount::delete_by_id(id.to_owned(), tx).await)
                .await?;
        }

        // Clear the user ID.
        self.user_id = None;

        Ok(())
    }

    async fn get_session_id(&self, user_id: &UserId) -> Result<Option<SessionId>, StoreError> {
        info!("getting user auth UID from store");

        let tether = self.stash.connection().await?;
        let sessions = CoreSession::find_by_user_id(user_id.to_owned(), &tether).await?;
        let session_id = sessions.into_iter().next().map(|s| s.remote_id);

        Ok(session_id)
    }
}

pub trait DecryptExt
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
