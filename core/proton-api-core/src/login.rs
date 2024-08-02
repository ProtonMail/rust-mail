#![allow(clippy::module_name_repetitions)]

use crate::auth::{Auth, StoreError, UserKeySecret};
use crate::service::{ApiServiceError, ServiceError};
use crate::services::proton::request_data::HumanVerificationData;
use crate::services::proton::requests::PostAuthRequest;
use crate::services::proton::response_data::{
    HumanVerificationChallenge, PasswordMode, TfaStatus, User,
};
use crate::session::{CoreSession, Session};
use core::fmt;
use proton_crypto_account::keys::{DecryptedUserKey, KeyId, LockedKey, UnlockResult};
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync as PgpProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider as SrpProvider;
use proton_crypto_account::salts::{KeySalt, KeySecret, Salt, Salts};
use secrecy::{ExposeSecret, SecretVec};
use std::fmt::{Debug, Formatter};
use thiserror::Error;

/// TODO: Document this enum.
#[derive(Debug, Error)]
pub enum LoginError {
    /// TODO: Document this variant.
    #[error("Human Verification Required'")]
    HumanVerificationRequired(HumanVerificationChallenge),

    /// TODO: Document this variant.
    #[error("Operation is not valid in the current state")]
    InvalidState,

    /// TODO: Document this variant.
    #[error("Failed to store the key secret in the authentication state: {0}")]
    KeySecretAuthUpdate(String),

    /// TODO: Document this variant.
    #[error("Failed to decrypt a user key with the derived client secret")]
    KeySecretDecryption,

    /// TODO: Document this variant.
    #[error("Failed to derive the key secret from the password: {0}")]
    KeySecretDerivation(#[from] SaltError),

    /// TODO: Document this variant.
    #[error("Failed to fetch salt to derive the key secret: {0}")]
    KeySecretSaltFetch(#[from] ApiServiceError),

    /// TODO: Document this variant.
    #[error("Server SRP proof verification failed: {0}")]
    ServerProof(String),

    /// TODO: Document this variant.
    #[error("Failed to calculate SRP Proof: {0}")]
    SrpProof(String),

    /// TODO: Document this variant.
    #[error("Account 2FA method is not supported")]
    UnsupportedTfa,

    /// TODO: Document this variant.
    #[error("Wrong mailbox password provided")]
    WrongMailboxPassword,

    /// Authentication Store operation failed.
    #[error("Authentication Store error: {0}")]
    AuthStore(#[from] StoreError),
}

impl ServiceError for LoginError {}

/// Handle all the possible states that are required to transition through in order to become
/// authenticated.

pub struct Flow {
    session: Session,
    state: LoginState,
    user: Option<User>,
    mailbox_password: Option<SecretVec<u8>>,
    password_mode: Option<PasswordMode>,
    tfa_status: Option<TfaStatus>,
}

impl Flow {
    #[must_use]
    pub fn new(session: Session) -> Self {
        Self {
            session,
            state: LoginState::LoggedOut,
            user: None,
            mailbox_password: None,
            password_mode: None,
            tfa_status: None,
        }
    }

    /// Start login with credentials. The `human_verification` parameter only needs to be submitted
    /// if during the login flow you catch a [`LoginError::HumanVerificationRequired`] error.
    ///
    /// # Errors
    /// Returns error if the login request or SRP proof calculations failed.
    pub async fn login(
        &mut self,
        username: String,
        password: String,
        human_verification: Option<HumanVerificationData>,
    ) -> Result<(), LoginError> {
        if !(self.is_logged_out() || human_verification.is_some()) {
            return Err(LoginError::InvalidState);
        }

        // We persist the password for the duration of the the login flow.
        self.mailbox_password = Some(SecretVec::new(password.as_bytes().to_vec()));

        let auth_resp = { self.session.api().post_auth_info(username.clone()).await }?;

        let srp_provider = proton_crypto_account::proton_crypto::new_srp_provider();
        let proof = srp_provider
            .generate_client_proof(
                &username,
                &password,
                auth_resp.version,
                &auth_resp.salt,
                &auth_resp.modulus,
                &auth_resp.server_ephemeral,
            )
            .map_err(|e| LoginError::SrpProof(e.to_string()))?;

        let auth_response = self
            .session
            .api()
            .post_auth(
                PostAuthRequest {
                    client_ephemeral: proof.ephemeral.clone(),
                    client_proof: proof.proof.clone(),
                    srp_session: auth_resp.srp_session,
                    username: username.clone(),
                },
                human_verification,
            )
            .await?;

        let skip_srp_proof_validation = self.session.api().config().skip_srp_proof_validation;

        if !skip_srp_proof_validation && !proof.compare_server_proof(&auth_response.server_proof) {
            return Err(LoginError::ServerProof(
                "Server Proof does not match".to_owned(),
            ));
        }

        {
            let auth = Auth {
                email: username,
                user_id: auth_response.user_id,
                uid: auth_response.uid,
                refresh_token: auth_response.refresh_token,
                access_token: auth_response.access_token,
                scope: auth_response.scope,
                key_secret: None,
            };

            self.session.auth_store().write().await.set(auth).await?;
        }
        self.tfa_status = Some(auth_response.tfa.enabled);
        self.password_mode = Some(auth_response.password_mode);
        self.next().await
    }

    /// Submit TOTP 2FA code.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn submit_totp(&mut self, code: String) -> Result<(), LoginError> {
        let LoginState::AwaitingTfa(status) = self.state else {
            return Err(LoginError::InvalidState);
        };

        if !matches!(status, TfaStatus::Totp | TfaStatus::TotpOrFido2) {
            return Err(LoginError::UnsupportedTfa);
        }

        self.session.api().post_auth_tfa(code).await?;

        self.next().await
    }

    /// Submit the second mailbox password in two password mode.
    ///
    /// # Errors
    /// Returns error if the request failed.
    /// If the password fails to decrypt the user key it returns a [`LoginError::WrongMailboxPassword`].
    pub async fn submit_mailbox_password(
        &mut self,
        mailbox_password: &str,
    ) -> Result<(), LoginError> {
        let LoginState::AwaitingMailboxPassword = self.state else {
            return Err(LoginError::InvalidState);
        };

        self.mailbox_password = Some(SecretVec::new(mailbox_password.as_bytes().to_vec()));
        let result = self.finalize_login().await;
        if matches!(result, Err(LoginError::KeySecretDecryption)) {
            return Err(LoginError::WrongMailboxPassword);
        }
        result?;

        self.next().await
    }

    /// Check whether the session has logged in.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        matches!(self.state, LoginState::LoggedIn)
    }

    /// Check whether the session in logged out.
    #[must_use]
    pub fn is_logged_out(&self) -> bool {
        matches!(self.state, LoginState::LoggedOut)
    }

    /// Check whether the session is awaiting totp.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        matches!(self.state, LoginState::AwaitingTfa(_))
    }

    /// Check whether the session is awaiting a mailbox password.
    ///
    /// If the user is in two password mode the mailbox password has to be provided separately.
    #[must_use]
    pub fn is_awaiting_mailbox_password(&self) -> bool {
        matches!(self.state, LoginState::AwaitingMailboxPassword)
    }

    /// Get the user info from a logged-in session and reset the internal state.
    pub fn reset_and_take_user(&mut self) -> Option<User> {
        self.state = LoginState::LoggedOut;
        self.password_mode = None;
        self.tfa_status = None;
        self.mailbox_password = None;
        self.user.take()
    }

    #[must_use]
    pub fn user(&self) -> Option<&User> {
        self.user.as_ref()
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Advances the internal state machine.
    async fn next(&mut self) -> Result<(), LoginError> {
        loop {
            match self.state {
                LoginState::LoggedOut => {
                    let Some(tfa_enabled) = self.tfa_status.take() else {
                        return Err(LoginError::InvalidState);
                    };
                    if tfa_enabled == TfaStatus::None {
                        self.state = LoginState::DeriveKeySecret;
                    } else {
                        self.state = LoginState::AwaitingTfa(tfa_enabled);
                        break;
                    }
                }
                LoginState::DeriveKeySecret => {
                    let Some(mode) = &self.password_mode else {
                        return Err(LoginError::InvalidState);
                    };
                    match mode {
                        PasswordMode::One => {
                            self.finalize_login().await?;
                            self.state = LoginState::LoggedIn;
                        }
                        PasswordMode::Two => {
                            self.mailbox_password = None;
                            self.state = LoginState::AwaitingMailboxPassword;
                            break;
                        }
                    }
                }
                LoginState::AwaitingTfa(_) => {
                    self.state = LoginState::DeriveKeySecret;
                }
                LoginState::AwaitingMailboxPassword => {
                    self.state = LoginState::LoggedIn;
                }
                LoginState::LoggedIn => break,
            }
        }
        Ok(())
    }

    /// Finalize the login by fetching the user and deriving the key secret.
    async fn finalize_login(&mut self) -> Result<(), LoginError> {
        // Fetch user info at least once, some accounts trigger HV after login with first
        // API call.
        let user = self.session.api().get_users().await?.user;
        self.derive_key_secret(&user).await?;
        self.user = Some(user);
        Ok(())
    }

    /// Derive the key secret to unlock user keys.
    async fn derive_key_secret(&mut self, user: &User) -> Result<(), LoginError> {
        let srp_provider = proton_crypto_account::proton_crypto::new_srp_provider();
        let pgp_provider = proton_crypto_account::proton_crypto::new_pgp_provider();
        let Some(password) = self.mailbox_password.as_mut() else {
            return Err(LoginError::InvalidState);
        };

        // Fetch the salts to derive the key password.
        let salts = Salts::new(
            self.session
                .api()
                .get_keys_salts()
                .await
                .map_err(LoginError::KeySecretSaltFetch)?
                .key_salts
                .into_iter()
                .map(|salt| Salt {
                    id: KeyId(salt.id.to_string()),
                    key_salt: salt.key_salt.map(KeySalt::from),
                }),
        );

        // Derive the key secret to unlock the user keys.
        let key_secret = Self::salt_password(user, &srp_provider, &salts, password.expose_secret())
            .map(UserKeySecret)
            .map_err(LoginError::KeySecretDerivation)?;

        // Check that the key works
        let unlock_result =
            Self::unlock_encryption_keys(user, &pgp_provider, key_secret.expose_secret());
        if unlock_result.unlocked_keys.is_empty() {
            return Err(LoginError::KeySecretDecryption);
        }

        // Update the auth state with the derived user secret.
        let mut guard = self.session.auth_store().write().await;
        if let Some(mut auth) = guard.get().cloned() {
            auth.key_secret = Some(key_secret);
            guard.set(auth).await?;
        }

        // The password is no longer needed, erase it.
        self.mailbox_password = None;
        Ok(())
    }

    /// Get the user's primary encryption key.
    ///
    /// # Parameters
    ///
    /// * `user` - The user to get the primary encryption key for.
    ///
    #[must_use]
    pub fn primary_encryption_key(user: &User) -> Option<&LockedKey> {
        user.keys.0.iter().find(|&key| key.primary)
    }

    /// Salt a user password.
    ///
    /// # Parameters
    ///
    /// * `user`             - The user to get the primary encryption key for.
    /// * `provider`         - TODO: Document this parameter.
    /// * `salts`            - TODO: Document this parameter.
    /// * `mailbox_password` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// If the primary encryption key is not found, a
    /// [`SaltError::PrimaryKeyNotFound`] is returned. Otherwise, any errors
    /// from [`Salts::salt_for_key()`] are propagated.
    ///
    pub fn salt_password<P: SrpProvider>(
        user: &User,
        provider: &P,
        salts: &Salts,
        mailbox_password: impl AsRef<[u8]>,
    ) -> Result<KeySecret, SaltError> {
        let Some(primary_key) = Self::primary_encryption_key(user) else {
            return Err(SaltError::PrimaryKeyNotFound);
        };
        salts
            .salt_for_key(provider, &primary_key.id, mailbox_password.as_ref())
            .map_err(|err| SaltError::Salt(err.to_string()))
    }

    /// Unlock the user's encryption keys.
    ///
    /// # Parameters
    ///
    /// * `user`            - The user to get the primary encryption key for.
    /// * `provider`        - TODO: Document this parameter.
    /// * `salted_password` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// See [`UserKeys::unlock()`](proton_crypto_account::keys::UserKeys::unlock()).
    ///
    pub fn unlock_encryption_keys<P: PgpProviderSync>(
        user: &User,
        provider: &P,
        salted_password: &KeySecret,
    ) -> UnlockResult<DecryptedUserKey<<P>::PrivateKey, <P>::PublicKey>> {
        user.keys.unlock(provider, salted_password)
    }
}

#[derive(Debug)]
enum LoginState {
    LoggedOut,
    AwaitingTfa(TfaStatus),
    DeriveKeySecret,
    AwaitingMailboxPassword,
    LoggedIn,
}

impl Debug for Flow {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "LoginFlow(state:{:?})", self.state)
    }
}

/// TODO: Document this enum.
#[derive(Debug, Error)]
pub enum SaltError {
    /// TODO: Document this variant.
    #[error("{0}")]
    Key(String),

    /// TODO: Document this variant.
    #[error("Could not find primary key")]
    PrimaryKeyNotFound,

    /// TODO: Document this variant.
    #[error("{0}")]
    Salt(String),
}
