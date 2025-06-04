use std::sync::Arc;

use datatypes::MigrationData;
use muon::client::flow::LoginExtraInfo;
use proton_account_api::login as login_api;
use tokio::{sync::Mutex, task::JoinError};
use uniffi::Enum as UniffiEnum;
use uniffi_common::errors::UserApiServiceError;
use uniffi_runtime::{async_runtime, uniffi_async};

mod datatypes;

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
    flow: Arc<Mutex<login_api::LoginFlow>>,
}

impl LoginFlow {
    #[must_use]
    pub fn new(flow: login_api::LoginFlow) -> Arc<Self> {
        Arc::new(Self {
            flow: Arc::new(Mutex::new(flow)),
        })
    }
}

#[uniffi_export]
impl LoginFlow {
    /// Login with user, password and optional fingerprints payload (for anti-abuse).
    /// * `fingerprint_payload` - a JSON array of objects serialized to a `String`.
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

        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .login(email, password, extra_info)
                .await
                .map_err(LoginError::from)
        })
        .await
        .into()
    }

    /// # Warning
    ///
    /// Should be used **only** to migrate existing sessions from legacy (pre-ET) version
    /// of the app. Used to prevent users from being logged-out after the update
    ///
    pub async fn migrate(&self, data: MigrationData) -> Result<(), LoginError> {
        let flow = self.flow.clone();

        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            let (user, data, refresh_token) = data.into_parts();
            guard
                .migrate(user, data, refresh_token)
                .await
                .map_err(LoginError::from)
        })
        .await
        .into()
    }

    /// Submit 2FA totp code.
    pub async fn submit_totp(&self, code: String) -> Result<(), LoginError> {
        let flow = self.flow.clone();
        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard.submit_totp(code).await.map_err(LoginError::from)
        })
        .await
        .into()
    }

    /// Submit mailbox password.
    pub async fn submit_mailbox_password(
        &self,
        mailbox_password: String,
    ) -> Result<(), LoginError> {
        let flow = self.flow.clone();
        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .submit_mailbox_password(mailbox_password)
                .await
                .map_err(LoginError::from)
        })
        .await
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
        async_runtime().block_on(async {
            self.flow
                .lock()
                .await
                .user_id()
                .map(|id| id.to_owned().into_inner())
                .map_err(LoginError::from)
        })
    }

    /// Get the session ID that has been (or is in the process of) being created.
    ///
    /// This can be used to resume a login flow.
    #[must_use]
    pub fn session_id(&self) -> Result<String, LoginError> {
        async_runtime().block_on(async {
            self.flow
                .lock()
                .await
                .session_id()
                .map(|id| id.to_owned().into_inner())
                .map_err(LoginError::from)
        })
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
}
impl LoginFlow {
    #[must_use]
    pub fn inner_flow(&self) -> Arc<Mutex<login_api::LoginFlow>> {
        Arc::clone(&self.flow)
    }
}

#[derive(Debug, UniffiEnum)]
pub enum LoginError {
    /// TODO: Document this variant.
    InvalidState,

    /// Returned if the initial auth request fails.
    FlowLogin(UserApiServiceError),

    /// Returned if the TOTP code submission fails.
    FlowTotp(UserApiServiceError),

    /// Returned if the FIDO2 challenge response fails.
    FlowFido(UserApiServiceError),

    /// Returned if we fail to fetch the user info after login.
    UserFetch(UserApiServiceError),

    /// Returned if the user keyring is invalid.
    MissingPrimaryKey,

    /// TODO: Document this variant.
    KeySecretDecryption,

    /// TODO: Document this variant.
    KeySecretDerivation(String),

    /// TODO: Document this variant.
    KeySecretSaltFetch(UserApiServiceError),

    /// TODO: Document this variant.
    ServerProof(String),

    /// TODO: Document this variant.
    SrpProof(String),

    /// TODO: Document this variant.
    WrongMailboxPassword,

    /// Authentication Store operation failed.
    AuthStore(String),

    Other(String),
}

impl From<login_api::LoginError> for LoginError {
    fn from(value: login_api::LoginError) -> Self {
        match value {
            login_api::LoginError::InvalidState => LoginError::InvalidState,
            login_api::LoginError::FlowLogin(e) => LoginError::FlowLogin(e.into()),
            login_api::LoginError::FlowTotp(e) => LoginError::FlowTotp(e.into()),
            login_api::LoginError::FlowFido(e) => LoginError::FlowFido(e.into()),
            login_api::LoginError::UserFetch(e) => LoginError::UserFetch(e.into()),
            login_api::LoginError::MissingPrimaryKey => LoginError::MissingPrimaryKey,
            login_api::LoginError::KeySecretDecryption => LoginError::KeySecretDecryption,
            login_api::LoginError::KeySecretDerivation(salt_error) => {
                LoginError::KeySecretDerivation(salt_error.to_string())
            }
            login_api::LoginError::KeySecretSaltFetch(e) => {
                LoginError::KeySecretSaltFetch(e.into())
            }
            login_api::LoginError::ServerProof(e) => LoginError::ServerProof(e.to_string()),
            login_api::LoginError::SrpProof(e) => LoginError::SrpProof(e.to_string()),
            login_api::LoginError::WrongMailboxPassword => LoginError::WrongMailboxPassword,
            login_api::LoginError::AuthStore(error) => LoginError::AuthStore(error.to_string()),
        }
    }
}

impl From<JoinError> for LoginError {
    fn from(value: JoinError) -> Self {
        Self::Other(value.to_string())
    }
}
