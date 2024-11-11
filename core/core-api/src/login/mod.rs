#![allow(clippy::module_name_repetitions)]

use crate::login::state::State;
use crate::service::{ApiServiceError, ServiceError};
use crate::services::proton::prelude::*;
use crate::session::Session;
use crate::store::StoreError;
use futures::TryFutureExt;
use std::fmt::Debug;
use thiserror::Error;

/// Alias the SaltError as our own.
pub type SaltError = proton_crypto_account::salts::SaltError;

/// Implements the possible states that the login flow can be in.
mod state;

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

/// A login flow that can be used to log in a user.
///
/// The flow is used to guide the user through the login process,
/// ensuring that all necessary steps are completed in the correct order.
#[derive(Debug)]
pub struct Flow {
    state: Option<State>,
}

impl Flow {
    #[must_use]
    pub fn new(session: Session) -> Self {
        let (client, store) = session.into_parts();

        Self {
            state: Some(State::want_login(client, store)),
        }
    }

    /// Resume the login flow at the 2FA step.
    #[must_use]
    pub fn resume_second_factor(
        session: Session,
        user_id: RemoteId,
        session_id: RemoteId,
        _: TfaStatus,
    ) -> Self {
        let (client, store) = session.into_parts();

        Self {
            state: Some(State::want_tfa_resume(client, store, user_id, session_id)),
        }
    }

    /// Resume the login flow at the mailbox password step.
    #[must_use]
    pub fn resume_mailbox_password(
        session: Session,
        user_id: RemoteId,
        session_id: RemoteId,
        _: PasswordMode,
    ) -> Self {
        let (client, store) = session.into_parts();

        Self {
            state: Some(State::want_mbp_resume(client, store, user_id, session_id)),
        }
    }

    /// Start login with credentials. The `human_verification` parameter only needs to be submitted
    /// if during the login flow you catch a [`LoginError::HumanVerificationRequired`] error.
    ///
    /// # Errors
    /// Returns error if the login request or SRP proof calculations failed.
    pub async fn login(&mut self, user: String, pass: String) -> Result<(), LoginError> {
        self.state = (self.state.take())
            .ok_or(LoginError::InvalidState)?
            .login(user, pass)
            .ok_into()
            .await?;

        Ok(())
    }

    /// Submit TOTP 2FA code.
    ///
    /// # Errors
    ///
    /// Returns error if the request failed.
    pub async fn submit_totp(&mut self, code: String) -> Result<(), LoginError> {
        self.state = (self.state.take())
            .ok_or(LoginError::InvalidState)?
            .submit_totp(code)
            .ok_into()
            .await?;

        Ok(())
    }

    /// Submit the second mailbox password in two password mode.
    ///
    /// # Errors
    ///
    /// Returns error if the request failed.
    /// If the password fails to decrypt the user key it returns a [`LoginError::WrongMailboxPassword`].
    pub async fn submit_mailbox_password(&mut self, pass: String) -> Result<(), LoginError> {
        self.state = (self.state.take())
            .ok_or(LoginError::InvalidState)?
            .submit_mbp(pass)
            .ok_into()
            .await?;

        Ok(())
    }

    /// Take the completed session from the flow.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow is incomplete.
    pub fn take_session(&mut self) -> Result<Session, LoginError> {
        let session = (self.state.take())
            .ok_or(LoginError::InvalidState)?
            .into_session()?;

        Ok(session)
    }

    /// Check whether the session in logged out.
    #[must_use]
    pub fn is_logged_out(&self) -> bool {
        matches!(self.state, Some(State::WantLogin(_)))
    }

    /// Check whether the session is awaiting totp.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        matches!(self.state, Some(State::WantTfa(_)))
    }

    /// Check whether the session is awaiting a mailbox password.
    ///
    /// If the user is in two password mode the mailbox password has to be provided separately.
    #[must_use]
    pub fn is_awaiting_mailbox_password(&self) -> bool {
        matches!(self.state, Some(State::WantMbp(_)))
    }

    /// Check whether the session has logged in.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        matches!(self.state, Some(State::Complete(_)))
    }

    /// Get the ID of the user that has been (or is about to be) logged in.
    ///
    /// # Errors
    ///
    /// Returns an error if the user ID is not yet known.
    pub fn user_id(&self) -> Result<&RemoteId, LoginError> {
        self.state
            .as_ref()
            .ok_or(LoginError::InvalidState)?
            .user_id()
    }

    /// Get the ID of the session that has been (or is about to be) logged in.
    ///
    /// # Errors
    ///
    /// Returns an error if the session ID is not yet known.
    pub fn session_id(&self) -> Result<&RemoteId, LoginError> {
        self.state
            .as_ref()
            .ok_or(LoginError::InvalidState)?
            .auth_id()
    }
}
