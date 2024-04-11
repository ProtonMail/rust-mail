use crate::mail::{MailSessionResult, MailUserSession};
use proton_mail_common as pmc;
use proton_mail_common::exports::thiserror;
use proton_mail_common::proton_api_mail::proton_api_core::domain::{
    ExposeSecret, HumanVerification, SecretString, TwoFactorAuth,
};
use proton_mail_common::proton_api_mail::proton_api_core::http::RequestError;
use proton_mail_common::proton_api_mail::proton_api_core::login::Flow as CoreLoginFlow;
use std::sync::Arc;
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
    flow: Arc<proton_async::sync::Mutex<CoreLoginFlow>>,
    ctx: pmc::MailContext,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum LoginFlowError {
    #[error("{0}")]
    Request(#[source] RequestError),
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

pub type LoginFlowResult<T> = Result<T, LoginFlowError>;

impl LoginFlow {
    pub(crate) fn new(flow: CoreLoginFlow, ctx: pmc::MailContext) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(proton_async::sync::Mutex::new(flow)),
            ctx,
        })
    }
}

#[uniffi::export]
impl LoginFlow {
    /// Login with user and password.
    pub async fn login(&self, email: String, password: String) -> LoginFlowResult<()> {
        let flow = self.flow.clone();
        let password = SecretString::new(password);
        let handle = self.ctx.async_runtime().spawn(async move {
            let mut guard = flow.lock().await;
            guard.login(&email, password.expose_secret(), None).await
        });
        handle.await.map_err(|e| {
            LoginFlowError::Request(RequestError::Other(anyhow!(
                "failed to join task handle {e}"
            )))
        })??;
        Ok(())
    }

    /// Submit 2FA totp code.
    pub async fn submit_totp(&self, code: String) -> LoginFlowResult<()> {
        let flow = self.flow.clone();
        let handle = self.ctx.async_runtime().spawn(async move {
            let mut guard = flow.lock().await;
            guard.submit_totp(&code).await
        });
        handle.await.map_err(|e| {
            LoginFlowError::Request(RequestError::Other(anyhow!(
                "failed to join task handle {e}"
            )))
        })??;
        Ok(())
    }

    /// Check whether the login flow has completed.
    pub fn is_logged_in(&self) -> bool {
        self.ctx
            .async_runtime()
            .block_on(async { self.flow.lock().await.is_logged_in() })
    }

    /// Check whether the login flow is awaiting 2FA input.
    pub fn is_awaiting_2fa(&self) -> bool {
        self.ctx
            .async_runtime()
            .block_on(async { self.flow.lock().await.is_awaiting_2fa() })
    }

    /// When the flow is considered logged in, transform it into a MailUserContext.
    pub fn to_user_context(&self) -> MailSessionResult<Arc<MailUserSession>> {
        self.ctx.async_runtime().block_on(async {
            let guard = self.flow.lock().await;
            let user_ctx = self.ctx.user_context_from_login_flow(&guard)?;
            Ok(MailUserSession::new(user_ctx))
        })
    }
}

impl From<proton_mail_common::proton_api_mail::proton_api_core::login::Error> for LoginFlowError {
    fn from(value: proton_mail_common::proton_api_mail::proton_api_core::login::Error) -> Self {
        use proton_mail_common::proton_api_mail::proton_api_core::login::Error as LFE;
        match value {
            LFE::Request(e) => LoginFlowError::Request(e),
            LFE::ServerProof(e) => LoginFlowError::ServerProof(e),
            LFE::Unsupported2FA(e) => LoginFlowError::Unsupported2FA(e),
            LFE::HumanVerificationRequired(e) => LoginFlowError::HumanVerificationRequired(e),
            LFE::SRPProof(e) => LoginFlowError::ServerProof(e),
            LFE::InvalidState => LoginFlowError::InvalidState,
        }
    }
}
