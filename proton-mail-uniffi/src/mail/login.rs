use crate::mail::{MailSessionResult, MailUserSession};
use futures::executor::block_on;
use proton_api_core::auth::{ExposeSecret, SecretString, StoreError};
use proton_api_core::login::Flow as CoreLoginFlow;
use proton_api_core::login::LoginError as RealLoginFlowError;
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::response_data::HumanVerificationChallenge;
use std::sync::Arc;
use tokio::spawn;
use tokio::sync::Mutex;
use uniffi::deps::anyhow::anyhow;

/// Flow through the required steps to authenticate and login a user.
///
/// The first stage of the login is the submission of the user credentials with [`LoginFlow::login`].
/// If this stage succeeds, you can check if the user needs to submit a 2FA token with
/// [`LoginFlow::is_awaiting_2fa`].
///
/// If the flow is awaiting a 2FA token, call [`LoginFlow::submit_totp`] with respective code.
///
/// Finally, when the user is logged in, [`LoginFlow::is_logged_in`] will return true and
/// the flow can be converted into a user session with [`LoginFlow::to_user_context`].
///
/// # Human Verification
/// If at any stage during the login human verification is requested, the requests will fail with
/// the [`LoginFlowError::HumanVerificationRequired`] error. If this happens, the process should
/// be repeated.
///
#[derive(uniffi::Object)]
pub struct LoginFlow {
    flow: Arc<Mutex<CoreLoginFlow>>,
    ctx: proton_mail_common::MailContext,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LoginFlowError {
    #[error("{0}")]
    Request(#[source] ApiServiceError),
    #[error("Server SRP proof verification failed: {0}")]
    ServerProof(String),
    #[error("Account 2FA method is not supported")]
    UnsupportedTfa,
    #[error("Human Verification Required'")]
    HumanVerificationRequired(HumanVerificationChallenge),
    #[error("Failed to calculate SRP Proof: {0}")]
    SrpProof(String),
    #[error("Operation is not valid in the current state")]
    InvalidState,
    #[error("Failed to derive the key secret from the password: {0}")]
    KeySecretDerivation(anyhow::Error),
    #[error("Failed to fetch salt to derive the key secret: {0}")]
    KeySecretSaltFetch(#[from] ApiServiceError),
    #[error("Failed to store the key secret in the authentication state: {0}")]
    KeySecretAuthUpdate(String),
    #[error("Failed to decrypt a user key with the derived client secret")]
    KeySecretDecryption,
    #[error("Wrong mailbox password provided")]
    WrongMailboxPassword,
    #[error("Authentication Store error: {0}")]
    AuthStore(#[from] StoreError),
}

pub type LoginFlowResult<T> = Result<T, LoginFlowError>;

impl LoginFlow {
    pub(crate) fn new(flow: CoreLoginFlow, ctx: proton_mail_common::MailContext) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(Mutex::new(flow)),
            ctx,
        })
    }
}

pub enum LoginResult {}

#[uniffi::export]
impl LoginFlow {
    /// Login with user and password.
    pub async fn login(&self, email: String, password: String) -> LoginFlowResult<()> {
        let flow = self.flow.clone();
        let password = SecretString::from(password);
        let handle = spawn(async move {
            let mut guard = flow.lock().await;
            guard
                .login(email, password.expose_secret().clone(), None)
                .await
        });
        handle.await.map_err(|e| {
            LoginFlowError::Request(ApiServiceError::UnknownError(format!(
                "failed to join task handle {e}"
            )))
        })??;
        Ok(())
    }

    /// Submit 2FA totp code.
    pub async fn submit_totp(&self, code: String) -> LoginFlowResult<()> {
        let flow = self.flow.clone();
        let handle = spawn(async move {
            let mut guard = flow.lock().await;
            guard.submit_totp(code).await
        });
        handle.await.map_err(|e| {
            LoginFlowError::Request(ApiServiceError::UnknownError(format!(
                "failed to join task handle {e}"
            )))
        })??;
        Ok(())
    }

    /// Check whether the login flow has completed.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        block_on(async { self.flow.lock().await.is_logged_in() })
    }

    /// Check whether the login flow is awaiting 2FA input.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        block_on(async { self.flow.lock().await.is_awaiting_2fa() })
    }

    /// When the flow is considered logged in, transform it into a `MailUserContext`.
    pub fn to_user_context(&self) -> MailSessionResult<Arc<MailUserSession>> {
        block_on(async {
            let guard = self.flow.lock().await;
            let user_ctx = self.ctx.user_context_from_login_flow(&guard).await?;
            Ok(MailUserSession::new(user_ctx))
        })
    }
}

impl From<RealLoginFlowError> for LoginFlowError {
    fn from(value: RealLoginFlowError) -> Self {
        match value {
            RealLoginFlowError::UnsupportedTfa => LoginFlowError::UnsupportedTfa,
            RealLoginFlowError::HumanVerificationRequired(e) => {
                LoginFlowError::HumanVerificationRequired(e)
            }
            RealLoginFlowError::ServerProof(e) | RealLoginFlowError::SrpProof(e) => {
                LoginFlowError::ServerProof(e)
            }
            RealLoginFlowError::InvalidState => LoginFlowError::InvalidState,
            RealLoginFlowError::KeySecretDerivation(e) => {
                LoginFlowError::KeySecretDerivation(anyhow!("{e}"))
            }
            RealLoginFlowError::KeySecretSaltFetch(e) => LoginFlowError::KeySecretSaltFetch(e),
            RealLoginFlowError::KeySecretAuthUpdate(e) => LoginFlowError::KeySecretAuthUpdate(e),
            RealLoginFlowError::KeySecretDecryption => LoginFlowError::KeySecretDecryption,
            RealLoginFlowError::WrongMailboxPassword => LoginFlowError::WrongMailboxPassword,
            RealLoginFlowError::AuthStore(e) => LoginFlowError::AuthStore(e),
        }
    }
}
