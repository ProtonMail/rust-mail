use crate::ApiError;
use crate::DelinquentState;
use crate::login::state::State;
use crate::shared::SecureString;
use crate::shared::challenge::{Behavior, ChallengeInfo};
use muon::client::flow::LoginFlowData;
use proton_core_api::service::{ApiServiceError, ServiceError};
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::{CoreSession, Session};
use proton_core_api::store::{StoreError, UserData};
use secrecy::SecretString;
use std::fmt::Debug;
use thiserror::Error;

/// Alias the `SaltError` as our own.
pub type SaltError = proton_crypto_account::salts::SaltError;

/// Implements the possible states that the login flow can be in.
pub mod state;

/// TODO: Document this enum.
#[derive(Debug, Error)]
pub enum LoginError {
    /// TODO: Document this variant.
    #[error("Operation is not valid in the current state")]
    InvalidState,

    /// Returned if the initial auth request fails.
    #[error("Failed to login: {0}")]
    FlowLogin(#[source] ApiServiceError),

    /// Returned if the TOTP code submission fails.
    #[error("Failed to submit TOTP code: {0}")]
    FlowTotp(#[source] ApiServiceError),

    /// Returned if the FIDO2 challenge response fails.
    #[error("Failed to submit FIDO2 challenge response: {0}")]
    FlowFido(#[source] ApiServiceError),

    /// Returned if the user is forbidden from logging in.
    #[error("User is forbidden from logging in")]
    NoLogin,

    /// Returned if the user has no proton address.
    #[error("User has no proton address")]
    NoAddress,

    /// Returned if we fail to fetch the user info after login.
    #[error("Failed to fetch user info: {0}")]
    UserFetch(#[source] ApiServiceError),

    /// Returned if we fail to fetch the user addresses after login.
    #[error("Failed to fetch user addresses: {0}")]
    AddressFetch(#[source] ApiServiceError),

    /// Returned if we fail to set up a new address.
    #[error("Failed to set up new address: {0}")]
    AddressSetup(String),

    /// Returned if we fail to setup the user key.
    #[error("Failed to setup user key: {0}")]
    UserKeySetup(String),

    /// Returned if we decide not to setup the user key.
    #[error("User key setup aborted")]
    UserKeySetupAborted,

    /// Returned if we fail to set up a new address key.
    #[error("Failed to set up new address key: {0}")]
    AddressKeySetup(String),

    /// Returned if we decide not to setup the address keys.
    #[error("Address key setup aborted")]
    AddressKeySetupAborted,

    /// Returned if the user keyring is invalid.
    #[error("Failed to find primary key in user keyring")]
    MissingPrimaryKey,

    /// TODO: Document this variant.
    #[error("Failed to decrypt a user key with the derived client secret")]
    KeySecretDecryption,

    /// TODO: Document this variant.
    #[error("Failed to derive the key secret from the password: {0}")]
    KeySecretDerivation(#[from] SaltError),

    /// TODO: Document this variant.
    #[error("Failed to fetch salt to derive the key secret: {0}")]
    KeySecretSaltFetch(#[source] ApiServiceError),

    /// TODO: Document this variant.
    #[error("Server SRP proof verification failed: {0}")]
    ServerProof(String),

    /// TODO: Document this variant.
    #[error("Failed to calculate SRP Proof: {0}")]
    SrpProof(String),

    /// Authentication Store operation failed.
    #[error("Authentication Store error: {0}")]
    AuthStore(#[from] StoreError),

    /// Authentication Store operation failed.
    #[error("ApiError: {0}")]
    ApiError(#[from] ApiError),

    #[error("Failed to poll the fork for completion: {0}")]
    WithCodePollFlowFailed(#[from] muon::Error),

    #[error("Failed during QR login encoding or encryption")]
    QRLoginEncoding,
}

impl ServiceError for LoginError {}

/// A login flow that can be used to log in a user.
///
/// The flow is used to guide the user through the login process,
/// ensuring that all necessary steps are completed in the correct order.
#[derive(Debug)]
pub struct LoginFlow {
    session: Session,
    state: State,
}

impl LoginFlow {
    /// Create a new login flow from the beginning.
    #[must_use]
    pub fn new(session: Session, challenge_info: ChallengeInfo) -> Self {
        let (client, parts) = session.to_parts();
        let state = State::new(client, parts, Some(challenge_info));

        Self { session, state }
    }

    /// Resume the login flow at the 2FA step.
    #[must_use]
    pub fn new_from_tfa(
        session: Session,
        user_id: UserId,
        session_id: SessionId,
        pass: Option<impl Into<SecureString>>,
    ) -> Self {
        let (client, parts) = session.to_parts();
        let state = State::new_from_tfa(client, parts, user_id, session_id, pass.map(Into::into));

        Self { session, state }
    }

    /// Resume the login flow at the mailbox password step.
    #[must_use]
    pub fn new_from_mbp(session: Session, user_id: UserId, session_id: SessionId) -> Self {
        let (client, parts) = session.to_parts();
        let state = State::new_from_mbp(client, parts, user_id, session_id);

        Self { session, state }
    }

    /// # WARNING
    ///
    /// This method is provided **only** to migrate existing sessions from legacy
    /// app into ET app.
    ///
    pub async fn migrate(
        &mut self,
        user: UserData,
        data: LoginFlowData,
        refresh_token: SecretString,
    ) -> Result<(), LoginError> {
        let (client, _) = self.session.to_parts();
        self.transition(|s: State| s.migrate(client, user, data, refresh_token))
            .await
            .inspect_err(|_| self.try_recover())?;

        Ok(())
    }

    /// Start login with credentials while passing additional `info`.
    ///
    /// # Errors
    ///
    /// Returns error if the login request or SRP proof calculations failed.
    pub async fn login_with_credentials(
        &mut self,
        user: String,
        pass: impl Into<SecureString>,
        user_behavior: Option<Behavior>,
    ) -> Result<(), LoginError> {
        self.transition(|s: State| s.login_with_credentials(user, pass.into(), user_behavior))
            .await
            .inspect_err(|_| self.try_recover())
    }

    /// Generates a QR code for user sign-in, optionally including an encryption key.
    ///
    /// This method initiates a code-based authentication flow and constructs a QR code string
    /// in the format: `version:user_code:encryption_key_base64:client_id`.
    /// If an encryption key is required, a secure 32-byte key is generated and encoded in Base64.
    /// The resulting state includes the QR code, user code, and encryption key (if applicable) for further processing.
    ///
    /// # Arguments
    /// * `need_encryption_key` - If `true`, generates a 32-byte encryption key; otherwise, uses an empty default.
    pub async fn generate_sign_in_qr_code(
        &mut self,
        need_encryption_key: bool,
    ) -> Result<String, LoginError> {
        self.transition(|s: State| s.generate_sign_in_qr_code(need_encryption_key))
            .await
            .inspect_err(|_| self.try_recover())?;
        match &self.state {
            State::WantQrConfirmation(state) => Ok(state.qr_code.clone()),
            _ => Err(LoginError::InvalidState),
        }
    }

    /// Verifies host device confirmation for QR code login and completes the authentication process.
    ///
    /// This method waits for host device confirmation of the QR code login, decodes the payload using
    /// the provided encryption key, fetches user information, validates the passphrase, and stores user
    /// data. On success, it constructs a completed authentication state with session details.
    pub async fn check_host_device_confirmation(&mut self) -> Result<(), LoginError> {
        self.transition(|s: State| s.check_host_device_confirmation())
            .await
            .inspect_err(|_| self.try_recover())
    }

    /// Submit TOTP 2FA code.
    ///
    /// # Errors
    ///
    /// Returns error if the request failed.
    pub async fn submit_totp(&mut self, code: String) -> Result<(), LoginError> {
        self.transition(|s: State| s.submit_totp(code))
            .await
            .inspect_err(|_| self.try_recover())
    }

    /// Submit FIDO 2FA code.
    ///
    /// This function is not yet implemented.
    ///
    /// # Errors
    ///
    /// Once implemented, this function will return an error if the request failed.
    pub async fn submit_fido(&mut self, code: String) -> Result<(), LoginError> {
        self.transition(|s: State| s.submit_fido(code))
            .await
            .inspect_err(|_| self.try_recover())
    }

    /// Submit the second mailbox password in two password mode.
    ///
    /// # Errors
    ///
    /// Returns [`LoginError::KeySecretDecryption`] if the password cannot unlock the user key,
    /// or another variant of [`LoginError`] if the request failed.
    pub async fn submit_mailbox_password(
        &mut self,
        pass: impl Into<SecureString>,
    ) -> Result<(), LoginError> {
        self.transition(|s: State| s.submit_mbp(pass.into()))
            .await
            .inspect_err(|_| self.try_recover())
    }

    /// Take the completed session from the flow.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow is incomplete.
    pub fn take_session(&mut self) -> Result<Session, LoginError> {
        self.take_state().into_session()
    }

    /// Check whether the session in logged out.
    #[must_use]
    pub fn is_logged_out(&self) -> bool {
        matches!(self.state, State::WantLogin(_))
    }

    #[must_use]
    pub fn api(&self) -> &muon::Client {
        self.session.api()
    }

    /// Check whether the session is awaiting totp.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        matches!(self.state, State::WantTfa(_))
    }

    /// Check whether the session is awaiting a mailbox password.
    ///
    /// If the user is in two password mode the mailbox password has to be provided separately.
    #[must_use]
    pub fn is_awaiting_mailbox_password(&self) -> bool {
        matches!(self.state, State::WantMbp(_))
    }

    #[must_use]
    pub fn is_awaiting_host_device_confirmation(&self) -> bool {
        matches!(self.state, State::WantQrConfirmation(_))
    }

    /// Check whether the session has logged in.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        matches!(self.state, State::Complete(_))
    }

    /// Check whether password change is required for a logged in user
    pub fn password_change_required(&self) -> Result<bool, LoginError> {
        if let State::Complete(c) = &self.state {
            c.password_change_required().ok_or(LoginError::InvalidState)
        } else {
            Err(LoginError::InvalidState)
        }
    }

    /// Return delinquent state of the user
    pub fn delinquent_state(&self) -> Result<DelinquentState, LoginError> {
        if let State::Complete(c) = &self.state {
            c.delinquent_state().ok_or(LoginError::InvalidState)
        } else {
            Err(LoginError::InvalidState)
        }
    }

    /// Get the ID of the user that has been (or is about to be) logged in.
    ///
    /// # Errors
    ///
    /// Returns an error if the user ID is not yet known.
    pub fn user_id(&self) -> Result<&UserId, LoginError> {
        self.state.user_id()
    }

    /// Get the ID of the session that has been (or is about to be) logged in.
    ///
    /// # Errors
    ///
    /// Returns an error if the session ID is not yet known.
    pub fn session_id(&self) -> Result<&SessionId, LoginError> {
        self.state.session_id()
    }

    /// Try to transition the flow to the next state.
    async fn transition(
        &mut self,
        f: impl AsyncFnOnce(State) -> Result<State, (State, LoginError)>,
    ) -> Result<(), LoginError> {
        match f(self.take_state()).await {
            Ok(state) => {
                self.state = state;
                Ok(())
            }

            Err((state, err)) => {
                self.state = state;
                Err(err)
            }
        }
    }

    /// Try to recover from a failed transition.
    fn try_recover(&mut self) {
        let (client, parts) = self.session.to_parts();

        match self.take_state() {
            State::LoginRetry => {
                self.state = State::new(client, parts, None);
            }

            State::TfaRetry(user_id, session_id, pass) => {
                self.state = State::new_from_tfa(client, parts, user_id, session_id, pass);
            }

            State::MbpRetry(user_id, session_id) => {
                self.state = State::new_from_mbp(client, parts, user_id, session_id);
            }

            state => self.state = state,
        }
    }

    fn take_state(&mut self) -> State {
        std::mem::replace(&mut self.state, State::Invalid)
    }
}
