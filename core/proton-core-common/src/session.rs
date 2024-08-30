#![allow(dead_code)]
#![allow(clippy::struct_field_names)]

use crate::datatypes::AuthScopes;
use crate::db::session::{
    DecryptedUserSession, EncryptedAccessToken, EncryptedKeySecret, EncryptedRefreshToken,
    EncryptedUserSession, SessionEncryptionKey,
};
use crate::os::{KeyChain, KeyChainError};
use anyhow::Error as AnyhowError;
use proton_api_core::auth::{Auth, UserKeySecret};
use secrecy::{ExposeSecret, SecretString};
use stash::stash::Stash;
use std::sync::Arc;
use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum CoreSessionError {
    #[error("A Cryptography error occurred")]
    Crypto,
    #[error("Keychain Error: {0}")]
    KeyChain(#[from] KeyChainError),
    #[error("Keychain has no encryption key")]
    KeyChainHasNoKey,
    #[error("Other: {0}")]
    Other(AnyhowError),
}

/// Core session retrieves the session
pub(crate) struct Session {
    auth: Option<Auth>,
    encrypted_session: Option<EncryptedUserSession>,
    stash: Stash,
    keychain: Arc<dyn KeyChain>,
}

impl Session {
    pub(crate) fn new(
        session: Option<DecryptedUserSession>,
        stash: Stash,
        keychain: Arc<dyn KeyChain>,
    ) -> Self {
        let auth = session.map(decrypted_session_to_auth);
        Self {
            auth: auth.clone(),
            encrypted_session: None,
            stash,
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
        access: &SecretString,
        refresh: &SecretString,
    ) -> Result<(EncryptedAccessToken, EncryptedRefreshToken), CoreSessionError> {
        let access =
            EncryptedAccessToken::new(access, key).map_err(|_| CoreSessionError::Crypto)?;
        let refresh =
            EncryptedRefreshToken::new(refresh, key).map_err(|_| CoreSessionError::Crypto)?;
        Ok((access, refresh))
    }

    fn encrypt_key_secret(
        key: &SessionEncryptionKey,
        key_secret: &UserKeySecret,
    ) -> Result<EncryptedKeySecret, CoreSessionError> {
        EncryptedKeySecret::new(key_secret, key).map_err(|_| CoreSessionError::Crypto)
    }
}

// TODO: The code below is temporarily commented out because it does need to be
// TODO: retained in terms of functionality, but will be handled later. Although
// TODO: it is generally preferable to delete rather than comment-out, this
// TODO: serves as a reminder that storage of the auth status needs to be
// TODO: re-implemented as befits the new architecture.
// impl proton_api_core::auth::Store for Session {
//     fn get_auth(&self) -> Option<&Auth> {
//         self.auth.as_ref()
//     }
//
//     #[tracing::instrument(skip(self,auth), fields(uid = ?auth.uid, user_id= ?auth.user_id))]
//     fn set_auth(&mut self, auth: Auth) -> Result<(), Box<dyn Error>> {
//         let session_key = self.get_encryption_key().map_err(|e| {
//             error!("Failed to retrieve encryption key from keychain: {e}");
//             self.on_error(&e);
//             e
//         })?;
//
//         let (encrypted_access_token, encrypted_refresh_token) =
//             Self::encrypt_tokens(&session_key, &auth.access_token, &auth.refresh_token).map_err(
//                 |e| {
//                     error!("Failed to encrypt tokens");
//                     self.on_error(&e);
//                     Box::new(e)
//                 },
//             )?;
//
//         let encrypted_key_secret = auth
//             .key_secret
//             .as_ref()
//             .map(|key_secret| Self::encrypt_key_secret(&session_key, key_secret))
//             .transpose()?;
//
//         let mut encrypted_session = auth_to_encrypted_session(
//             auth.clone(),
//             encrypted_access_token,
//             encrypted_refresh_token,
//             encrypted_key_secret,
//         );
//         block_on(async { encrypted_session.save().await })?;
//         self.auth = Some(auth);
//         self.encrypted_session = Some(encrypted_session);
//         Ok(())
//     }
//
//     #[tracing::instrument(skip(self), fields(uid = ? uid))]
//     fn refresh_auth(
//         &mut self,
//         uid: RemoteId,
//         access_token: SecretString,
//         refresh_token: SecretString,
//         scope: String,
//     ) -> Result<(), Box<dyn Error>> {
//         let user_id = {
//             let Some(auth) = &self.auth else {
//                 return Err(Box::new(CoreSessionError::Other(anyhow!(
//                     "no auth into to refresh"
//                 ))));
//             };
//             auth.user_id.clone()
//         };
//         let session_key = self.get_encryption_key().map_err(|e| {
//             error!("Failed to retrieve encryption key from keychain: {e}");
//             self.on_error(&e);
//             e
//         })?;
//
//         let (encrypted_access_token, encrypted_refresh_token) =
//             Self::encrypt_tokens(&session_key, &access_token, &refresh_token).map_err(|e| {
//                 error!("Failed to encrypt tokens");
//                 self.on_error(&e);
//                 Box::new(e)
//             })?;
//
//         block_on(async {
//             let mut session = EncryptedUserSession::load(user_id.clone(), &self.stash)
//                 .await?
//                 .unwrap();
//             session.user_id = user_id;
//             session.access_token = encrypted_access_token;
//             session.refresh_token = encrypted_refresh_token;
//             session.scopes = scope.clone();
//             session.save().await
//         })?;
//
//         if let Some(cur_auth) = &mut self.auth {
//             cur_auth.uid = uid;
//             cur_auth.access_token = access_token;
//             cur_auth.refresh_token = refresh_token;
//             cur_auth.scope = scope;
//         }
//
//         Ok(())
//     }
//
//     fn refresh_user_key_secret(
//         &mut self,
//         user_key_secret: UserKeySecret,
//     ) -> Result<(), Box<dyn Error>> {
//         if self.auth.is_none() || self.encrypted_session.is_none() {
//             return Err(Box::new(CoreSessionError::Other(anyhow!(
//                 "no auth info found to refresh"
//             ))));
//         };
//         let session_key = self.get_encryption_key().map_err(|e| {
//             error!("Failed to retrieve encryption key from the keychain: {e}");
//             self.on_error(&e);
//             e
//         })?;
//
//         // Unwrap is safe here as we have checked for None above
//         let encrypted_session = self.encrypted_session.as_mut().unwrap();
//         let encrypted_key_secret = Self::encrypt_key_secret(&session_key, &user_key_secret)?;
//         encrypted_session.key_secret = Some(encrypted_key_secret.clone());
//         block_on(async { encrypted_session.save().await })?;
//
//         // Unwrap is safe here as we have checked for None above
//         self.auth.as_mut().unwrap().key_secret = Some(user_key_secret);
//         Ok(())
//     }
//
//     fn set_scopes(&mut self, _: String) -> Result<Option<&Auth>, Box<dyn Error>> {
//         todo!()
//     }
//
//     fn clear_auth(&mut self) -> Result<(), Box<dyn Error>> {
//         let Some(auth) = self.auth.take() else {
//             return Ok(());
//         };
//
//         tracing::debug_span!("clear_auth", uid=?auth.uid, ?auth.user_id).in_scope(
//             || -> Result<(), Box<dyn Error>> {
//                 debug!("Deleting session");
//                 block_on(async {
//                     self.stash
//                         .execute(
//                             "DELETE FROM core_sessions WHERE user_id =?",
//                             params![auth.user_id],
//                         )
//                         .await
//                 })?;
//                 Ok(())
//             },
//         )
//     }
// }

fn decrypted_session_to_auth(session: DecryptedUserSession) -> Auth {
    Auth {
        name_or_addr: session.name_or_addr,
        user_id: session.user_id.into(),
        uid: session.session_id.into(),
        refresh_token: session.refresh_token.into(),
        access_token: session.access_token.into(),
        scopes: session.scopes.into_inner(),
        key_secret: session.key_secret,
    }
}

#[deprecated(note = "This does not seem to be used anywhere")]
fn auth_to_encrypted_session(
    auth: Auth,
    encrypted_access_token: EncryptedAccessToken,
    encrypted_refresh_token: EncryptedRefreshToken,
    encrypted_key_secret: Option<EncryptedKeySecret>,
) -> EncryptedUserSession {
    EncryptedUserSession {
        session_id: auth.uid.into(),
        user_id: auth.user_id.into(),
        name_or_addr: auth.name_or_addr,
        refresh_token: encrypted_refresh_token,
        access_token: encrypted_access_token,
        key_secret: encrypted_key_secret,
        scopes: AuthScopes::new(auth.scopes),
        row_id: None,
        stash: None,
    }
}
