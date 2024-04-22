use crate::auth::Auth;
use crate::domain::{HumanVerification, LoginData, TFAStatus, TwoFactorAuth, User};
use crate::requests::{AuthInfo, TOTPRequest};
use crate::{http, Session};
use anyhow::anyhow;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
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
}

/// Handle all the possible states that are required to transition through in order to become
/// authenticated.

pub struct Flow {
    session: Session,
    state: LoginState,
    user: Option<User>,
}

impl Flow {
    #[must_use]
    pub fn new(session: Session) -> Self {
        Self {
            session,
            state: LoginState::LoggedOut,
            user: None,
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
        // TODO: This inequality comparison should be done in constant time once it is exposed by proton-crypto.
        if !skip_srp_proof_validation && proof.expected_server_proof != auth_response.server_proof {
            return Err(Error::ServerProof("Server Proof does not match".to_owned()));
        }

        let tfa_enabled = auth_response.tfa.enabled;
        {
            let auth = Auth {
                email: username.to_owned(),
                user_id: auth_response.user_id,
                uid: auth_response.uid,
                refresh_token: auth_response.refresh_token,
                access_token: auth_response.access_token,
                scope: auth_response.scope,
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

        if tfa_enabled != TFAStatus::None {
            self.state = LoginState::Awaiting2FA(tfa_enabled);
            return Ok(());
        }

        self.post_login_user_fetch().await
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

        self.post_login_user_fetch().await?;
        Ok(())
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

    /// Check whether the session in awaiting totp.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        matches!(self.state, LoginState::Awaiting2FA(_))
    }

    /// Get the user info from a logged-in session and reset the internal state.
    pub fn reset_and_take_user(&mut self) -> Option<User> {
        self.state = LoginState::LoggedOut;
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

    async fn post_login_user_fetch(&mut self) -> Result<(), Error> {
        // Fetch user info at least once, some accounts trigger HV after login with first
        // API call.
        let user = self
            .session
            .get_user()
            .await
            .map_err(map_human_verification_err)?;
        self.state = LoginState::LoggedIn;
        self.user = Some(user);
        Ok(())
    }
}

#[derive(Debug)]
enum LoginState {
    LoggedOut,
    Awaiting2FA(TFAStatus),
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
