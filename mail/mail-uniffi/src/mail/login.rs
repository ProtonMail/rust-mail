use crate::errors::{LoginError, VoidLoginResult};
use crate::mail::MailUserSession;
use crate::{async_runtime, uniffi_async};
use proton_api_core::login::Flow as CoreLoginFlow;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use std::sync::Arc;
use tokio::sync::Mutex;

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
    ctx: Arc<proton_mail_common::MailContext>,
}

impl LoginFlow {
    pub(crate) fn new(flow: CoreLoginFlow, ctx: Arc<proton_mail_common::MailContext>) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(Mutex::new(flow)),
            ctx,
        })
    }
}

#[uniffi::export]
impl LoginFlow {
    /// Login with user and password.
    pub async fn login(&self, email: String, password: String) -> VoidLoginResult {
        let flow = self.flow.clone();
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .login(email, password)
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(LoginError::from)
        .into()
    }

    /// Submit 2FA totp code.
    pub async fn submit_totp(&self, code: String) -> VoidLoginResult {
        let flow = self.flow.clone();
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .submit_totp(code)
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(LoginError::from)
        .into()
    }

    /// Submit mailbox password.
    pub async fn submit_mailbox_password(&self, mailbox_password: String) -> VoidLoginResult {
        let flow = self.flow.clone();
        uniffi_async::<_, RealProtonMailError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .submit_mailbox_password(mailbox_password)
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(LoginError::from)
        .into()
    }
}

#[proton_uniffi_macros::export_result]
impl LoginFlow {
    /// Check whether the login flow has completed.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_logged_in() })
    }

    /// Get the user ID of the user that has (or is in the process of) logging in.
    ///
    /// This can be used to resume a login flow.
    #[must_use]
    pub fn user_id(&self) -> Result<String, LoginError> {
        async_runtime()
            .block_on(async {
                self.flow
                    .lock()
                    .await
                    .user_id()
                    .map(|id| id.to_owned().into_inner())
                    .map_err(RealProtonMailError::from)
            })
            .map_err(LoginError::from)
    }

    /// Get the session ID that has been (or is in the process of) being created.
    ///
    /// This can be used to resume a login flow.
    #[must_use]
    pub fn session_id(&self) -> Result<String, LoginError> {
        async_runtime()
            .block_on(async {
                self.flow
                    .lock()
                    .await
                    .session_id()
                    .map(|id| id.to_owned().into_inner())
                    .map_err(RealProtonMailError::from)
            })
            .map_err(LoginError::from)
    }

    /// Check whether the login flow is awaiting 2FA input.
    #[must_use]
    pub fn is_awaiting_2fa(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_awaiting_2fa() })
    }

    /// Check whether the login flow is awaiting mailbox password input.
    #[must_use]
    pub fn is_awaiting_mailbox_password(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_awaiting_mailbox_password() })
    }

    /// When the flow is considered logged in, transform it into a `MailUserContext`.
    #[must_use]
    pub fn to_user_context(&self) -> Result<Arc<MailUserSession>, LoginError> {
        async_runtime()
            .block_on(async {
                let mut guard = self.flow.lock().await;
                let user_ctx = self
                    .ctx
                    .user_context_from_login_flow(&mut guard)
                    .await
                    .map_err(RealProtonMailError::from)?;
                Ok::<_, RealProtonMailError>(MailUserSession::new(user_ctx))
            })
            .map_err(LoginError::from)
    }
}
