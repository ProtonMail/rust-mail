use crate::auth::{Auth, UserKeySecret};
use crate::domain::{HumanVerification, LoginData, SaltError, TFAStatus, TwoFactorAuth, User};
use crate::requests::{AuthInfo, PasswordMode, TOTPRequest};
use crate::{http, Session};
use anyhow::anyhow;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use secrecy::{ExposeSecret, SecretVec};
use std::fmt::Formatter;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Request(#[source] http::RequestError),
    #[error("Server SRP proof verification failed: {0}")]
    ServerProof(String),
    #[error("Account 2FA method ({0})is not supported")]
    Unsupported2FA(TwoFactorAuth),
    #[error("Human Verification Required'")]
    HumanVerificationRequired(HumanVerification),
    #[error("Failed to calculate SRP Proof: {0}")]
    SRPProof(String),
    #[error("Operation is nto valid in the current state")]
    InvalidState,
    #[error("Failed to derive the key secret from the password: {0}")]
    KeySecretDerivation(#[from] SaltError),
    #[error("Failed to fetch salt to derive the key secret: {0}")]
    KeySecretSaltFetch(#[from] http::RequestError),
    #[error("Failed to store the key secret in the authentication state: {0}")]
    KeySecretAuthUpdate(String),
    #[error("Failed to decrypt a user key with the derived client secret")]
    KeySecretDecryption,
    #[error("Wrong mailbox password provided")]
    WrongMailboxPassword,
}

/// Handle all the possible states that are required to transition through in order to become
/// authenticated.

pub struct Flow {
    session: Session,
    state: LoginState,
    user: Option<User>,
    mailbox_password: Option<SecretVec<u8>>,
    password_mode: Option<PasswordMode>,
    tfa_status: Option<TFAStatus>,
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
    /// if during the login flow you catch a [`Error::HumanVerificationRequired`] error.
    ///
    /// # Errors
    /// Returns error if the login request or SRP proof calculations failed.
    pub async fn login<'a>(
        &mut self,
        username: &'a str,
        password: &'a str,
        human_verification: Option<LoginData>,
    ) -> Result<(), Error> {
        if !(self.is_logged_out() || human_verification.is_some()) {
            return Err(Error::InvalidState);
        }

        // We persist the password for the duration of the the login flow.
        self.mailbox_password = Some(SecretVec::new(password.as_bytes().to_vec()));

        let auth_resp = {
            self.session
                .execute_request(AuthInfo { username })
                .await
                .map_err(map_human_verification_err)
        }?;

        let srp_provider = proton_crypto_account::proton_crypto::new_srp_provider();
        let proof = srp_provider
            .generate_client_proof(
                username,
                password,
                auth_resp.version,
                &auth_resp.salt,
                &auth_resp.modulus,
                &auth_resp.server_ephemeral,
            )
            .map_err(|e| Error::SRPProof(e.to_string()))?;

        let auth_response = self
            .session
            .execute_request(crate::requests::Auth {
                username,
                client_ephemeral: &proof.ephemeral,
                client_proof: &proof.proof,
                srp_session: &auth_resp.srp_session,
                human_verification: &human_verification,
            })
            .await
            .map_err(map_human_verification_err)?;

        let skip_srp_proof_validation = self.session.api_env_config().skip_srp_proof_validation;

        if !skip_srp_proof_validation && !proof.compare_server_proof(&auth_response.server_proof) {
            return Err(Error::ServerProof("Server Proof does not match".to_owned()));
        }

        {
            let auth = Auth {
                email: username.to_owned(),
                user_id: auth_response.user_id,
                uid: auth_response.uid,
                refresh_token: auth_response.refresh_token,
                access_token: auth_response.access_token,
                scope: auth_response.scope,
                key_secret: None,
            };

            self.session
                .auth_store()
                .write()
                .await
                .set_auth(auth)
                .map_err(|e| {
                    Error::Request(http::RequestError::Other(anyhow!(
                        "Failed to to store auth: {e}"
                    )))
                })?;
        }
        self.tfa_status = Some(auth_response.tfa.enabled);
        self.password_mode = Some(auth_response.password_mode);
        self.next().await
    }

    /// Submit TOTP 2FA code.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn submit_totp(&mut self, code: &str) -> Result<(), Error> {
        let LoginState::Awaiting2FA(status) = self.state else {
            return Err(Error::InvalidState);
        };

        if !matches!(status, TFAStatus::Totp | TFAStatus::TotpOrFIDO2) {
            return Err(Error::Unsupported2FA(TwoFactorAuth::TOTP));
        }

        self.session
            .execute_request(TOTPRequest::new(code))
            .await
            .map_err(map_human_verification_err)?;

        self.next().await
    }

    /// Submit the second mailbox password in two password mode.
    ///
    /// # Errors
    /// Returns error if the request failed.
    /// If the password fails to decrypt the user key it returns a [`Error::WrongMailboxPassword`].
    pub async fn submit_mailbox_password(&mut self, mailbox_password: &str) -> Result<(), Error> {
        let LoginState::AwaitingMailboxPassword = self.state else {
            return Err(Error::InvalidState);
        };

        self.mailbox_password = Some(SecretVec::new(mailbox_password.as_bytes().to_vec()));
        let result = self.finalize_login().await;
        if matches!(result, Err(Error::KeySecretDecryption)) {
            return Err(Error::WrongMailboxPassword);
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
        matches!(self.state, LoginState::Awaiting2FA(_))
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
    async fn next(&mut self) -> Result<(), Error> {
        loop {
            match self.state {
                LoginState::LoggedOut => {
                    let Some(tfa_enabled) = self.tfa_status.take() else {
                        return Err(Error::InvalidState);
                    };
                    if tfa_enabled == TFAStatus::None {
                        self.state = LoginState::DeriveKeySecret;
                    } else {
                        self.state = LoginState::Awaiting2FA(tfa_enabled);
                        break;
                    }
                }
                LoginState::DeriveKeySecret => {
                    let Some(mode) = &self.password_mode else {
                        return Err(Error::InvalidState);
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
                LoginState::Awaiting2FA(_) => {
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
    async fn finalize_login(&mut self) -> Result<(), Error> {
        // Fetch user info at least once, some accounts trigger HV after login with first
        // API call.
        let user = self
            .session
            .get_user()
            .await
            .map_err(map_human_verification_err)?;
        self.derive_key_secret(&user).await?;
        self.user = Some(user);
        Ok(())
    }

    /// Derive the key secret to unlock user keys.
    async fn derive_key_secret(&mut self, user: &User) -> Result<(), Error> {
        let srp_provider = proton_crypto_account::proton_crypto::new_srp_provider();
        let pgp_provider = proton_crypto_account::proton_crypto::new_pgp_provider();
        let Some(password) = self.mailbox_password.as_mut() else {
            return Err(Error::InvalidState);
        };

        // Fetch the salts to derive the key password.
        let salts = self
            .session
            .get_user_salts()
            .await
            .map_err(Error::KeySecretSaltFetch)?;

        // Derive the key secret to unlock the user keys.
        let key_secret = user
            .salt_password(&srp_provider, &salts, password.expose_secret())
            .map(UserKeySecret)
            .map_err(Error::KeySecretDerivation)?;

        // Check that the key works
        let unlock_result = user.unlock_keys(&pgp_provider, key_secret.expose_secret());
        if unlock_result.unlocked_keys.is_empty() {
            return Err(Error::KeySecretDecryption);
        }

        // Update the auth state with the derived user secret.
        self.session
            .auth_store()
            .write()
            .await
            .refresh_user_key_secret(key_secret)
            .map_err(|e| {
                Error::Request(http::RequestError::Other(anyhow!(
                    "Failed to store auth with user secret: {e}"
                )))
            })?;

        // The password is no longer needed, erase it.
        self.mailbox_password = None;
        Ok(())
    }
}

#[derive(Debug)]
enum LoginState {
    LoggedOut,
    Awaiting2FA(TFAStatus),
    DeriveKeySecret,
    AwaitingMailboxPassword,
    LoggedIn,
}

fn map_human_verification_err(e: http::RequestError) -> Error {
    if let http::RequestError::API(e) = &e {
        if let Ok(hv) = e.try_get_human_verification_details() {
            return Error::HumanVerificationRequired(hv);
        }
    }

    Error::Request(e)
}

impl std::fmt::Debug for Flow {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoginFlow(state:{:?})", self.state)
    }
}
