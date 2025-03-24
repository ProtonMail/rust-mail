use crate::core::datatypes::MigrationData;
use crate::errors::{LoginError, VoidLoginResult};
use crate::mail::MailUserSession;
use crate::mail::state::MailUserContextMap;
use crate::{async_runtime, uniffi_async};
use futures::TryFutureExt;
use proton_api_core::login::Flow as CoreLoginFlow;
use proton_api_core::services::proton::muon::client::flow::LoginExtraInfo;
use proton_mail_common::MailContext;
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
    mail_ctx: Arc<MailContext>,
    user_ctx: Arc<MailUserContextMap>,
}

impl LoginFlow {
    pub(crate) fn new(
        flow: CoreLoginFlow,
        mail_ctx: Arc<MailContext>,
        user_ctx: Arc<MailUserContextMap>,
    ) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(Mutex::new(flow)),
            mail_ctx,
            user_ctx,
        })
    }
}

#[uniffi_export]
impl LoginFlow {
    /// Login with user, password and optional fingerprints payload (for anti-abuse).
    /// * `fingerprint_payload` - a JSON array of objects serialized to a `String`.
    #[returns(VoidLoginResult)]
    pub async fn login(
        &self,
        email: String,
        password: String,
        fingerprint_payload: Option<String>,
    ) -> Result<(), LoginError> {
        let flow = self.flow.clone();

        let fingerprint_result = fingerprint_payload.as_ref().map(|f| f.parse()).transpose();
        let extra_info = match fingerprint_result {
            Ok(Some(f)) => LoginExtraInfo::builder().with_fingerprint(f).build(),
            Ok(None) => LoginExtraInfo::default(),
            Err(_) => todo!(),
        };

        uniffi_async::<_, RealProtonMailError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .login(email, password, extra_info)
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(LoginError::from)
        .into()
    }

    /// # Warning
    ///
    /// Should be used **only** to migrate existing sessions from legacy (pre-ET) version
    /// of the app. Used to prevent users from being logged-out after the update
    ///
    pub async fn migrate(&self, data: MigrationData) -> Result<(), LoginError> {
        let flow = self.flow.clone();

        uniffi_async::<_, RealProtonMailError, _>(async move {
            let mut guard = flow.lock().await;
            let (user, data, refresh_token) = data.into_parts();
            guard
                .migrate(user, data, refresh_token)
                .await
                .map_err(RealProtonMailError::from)
        })
        .await
        .map_err(From::from)
        .into()
    }

    /// Submit 2FA totp code.
    #[returns(VoidLoginResult)]
    pub async fn submit_totp(&self, code: String) -> Result<(), LoginError> {
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
    #[returns(VoidLoginResult)]
    pub async fn submit_mailbox_password(
        &self,
        mailbox_password: String,
    ) -> Result<(), LoginError> {
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

#[uniffi_export]
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

                self.mail_ctx
                    .user_context_from_login_flow(&mut guard)
                    .map_ok(|ctx| self.user_ctx.insert(ctx))
                    .map_ok(MailUserSession::new)
                    .map_err(RealProtonMailError::from)
                    .await
            })
            .map_err(LoginError::from)
    }
}
