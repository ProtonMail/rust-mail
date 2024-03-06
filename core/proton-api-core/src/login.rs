use crate::auth::Auth;
use crate::domain::{
    HumanVerification, HumanVerificationLoginData, TFAStatus, TwoFactorAuth, User,
};
use crate::requests::{AuthInfoRequest, AuthRequest, TOTPRequest};
use crate::{http, Session};
use anyhow::anyhow;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use std::fmt::Formatter;

#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[cfg_attr(feature = "uniffi", uniffi(flat_error))]
pub enum LoginFlowError {
    #[error("{0}")]
    Request(#[source] http::HttpRequestError),
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

pub struct LoginFlow {
    session: Session,
    state: LoginState,
    user: Option<User>,
}

impl LoginFlow {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            state: LoginState::LoggedOut,
            user: None,
        }
    }

    /// Start login with credentials. The `human_verification` parameter only needs to be submitted
    /// if during the login flow you catch a [`LoginFlowError::HumanVerificationRequired`] error.
    pub async fn login<'a>(
        &mut self,
        username: &'a str,
        password: &'a str,
        human_verification: Option<HumanVerificationLoginData>,
    ) -> Result<(), LoginFlowError> {
        if !(self.is_logged_out() || human_verification.is_some()) {
            return Err(LoginFlowError::InvalidState);
        }

        let auth_resp = {
            self.session
                .execute_request(AuthInfoRequest { username })
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
            .map_err(|e| LoginFlowError::SRPProof(e.to_string()))?;

        let auth_response = self
            .session
            .execute_request(AuthRequest {
                username,
                client_ephemeral: &proof.ephemeral,
                client_proof: &proof.proof,
                srp_session: &auth_resp.srp_session,
                human_verification: &human_verification,
            })
            .await
            .map_err(map_human_verification_err)?;

        if proof.expected_server_proof != auth_response.server_proof {
            return Err(LoginFlowError::ServerProof(
                "Server Proof does not match".to_string(),
            ));
        }

        let tfa_enabled = auth_response.tfa.enabled;
        {
            let auth = Auth {
                email: username.to_string(),
                user_id: auth_response.user_id,
                uid: auth_response.uid,
                refresh_token: auth_response.refresh_token.0,
                access_token: auth_response.access_token.0,
                scope: auth_response.scope,
            };

            self.session
                .auth_store()
                .write()
                .await
                .set_auth(auth)
                .map_err(|e| {
                    LoginFlowError::Request(http::HttpRequestError::Other(anyhow!(
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
    pub async fn submit_totp(&mut self, code: &str) -> Result<(), LoginFlowError> {
        let LoginState::Awaiting2FA(status) = self.state else {
            return Err(LoginFlowError::InvalidState);
        };

        if !matches!(status, TFAStatus::Totp | TFAStatus::TotpOrFIDO2) {
            return Err(LoginFlowError::Unsupported2FA(TwoFactorAuth::TOTP));
        }

        self.session
            .execute_request(TOTPRequest::new(code))
            .await
            .map_err(map_human_verification_err)?;

        self.post_login_user_fetch().await?;
        Ok(())
    }

    /// Check whether the session has logged in.
    pub fn is_logged_in(&self) -> bool {
        matches!(self.state, LoginState::LoggedIn)
    }

    /// Check whether the session in logged out.
    pub fn is_logged_out(&self) -> bool {
        matches!(self.state, LoginState::LoggedOut)
    }

    /// Check whether the session in awaiting totp.
    pub fn is_awaiting_2fa(&self) -> bool {
        matches!(self.state, LoginState::Awaiting2FA(_))
    }

    /// Get the user info from a logged-in session and reset the internal state.
    pub fn reset_and_take_user(&mut self) -> Option<User> {
        self.state = LoginState::LoggedOut;
        self.user.take()
    }

    pub fn user(&self) -> Option<&User> {
        self.user.as_ref()
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    async fn post_login_user_fetch(&mut self) -> Result<(), LoginFlowError> {
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

fn map_human_verification_err(e: http::HttpRequestError) -> LoginFlowError {
    if let http::HttpRequestError::API(e) = &e {
        if let Ok(hv) = e.try_get_human_verification_details() {
            return LoginFlowError::HumanVerificationRequired(hv);
        }
    }

    LoginFlowError::Request(e)
}

impl std::fmt::Debug for LoginFlow {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoginFlow(state:{:?})", self.state)
    }
}
