use crate::{password_validator::PasswordValidatorService, user_behavior::UserBehavior};
use datatypes::{Fido2RequestFfi, Fido2ResponseFfi, MigrationData};
use muon::common::IntoDyn;
use proton_account_api::login as login_api;
use proton_account_api::login::state::want_qr_confirmation::ProcessTargetDeviceQrError as RealProcessTargetDeviceQrError;
use proton_account_api::responses as responses_api;
use proton_core_api::service::ApiServiceError;
use proton_core_common::post_login_check::PostLoginValidationError as RealPostLoginValidationError;
use std::sync::Arc;
use tokio::{sync::Mutex, task::JoinError};
use tracing::warn;
use uniffi::Enum as UniffiEnum;
use uniffi_common::errors::UserApiServiceError;
use uniffi_runtime::{async_runtime, uniffi_async};

pub mod datatypes;

/// Flow through the required steps to authenticate and login a user.
///
/// The first stage of the login is the submission of the user credentials with [`LoginFlow::login`].
/// If this stage succeeds, you can check if the user needs to submit a 2FA token with
/// [`LoginFlow::is_awaiting_2fa`].
///
/// If the flow is awaiting a 2FA token, call [`LoginFlow::submit_totp`] or
/// [`LoginFlow::submit_fido`] with respective code depending on the user choice and ability.
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
        username: String,
        password: String,
        user_behavior: Option<UserBehavior>,
    ) -> Result<(), LoginError> {
        let flow = self.flow.clone();

        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .login_with_credentials(username.as_str(), password, user_behavior.map(Into::into))
                .await
                .map_err(LoginError::from)
        })
        .await
        .into()
    }

    /// Get the FIDO2 details for authentication.
    #[must_use]
    pub async fn get_fido_details(&self) -> Result<Option<Fido2ResponseFfi>, LoginError> {
        let flow = self.flow.clone();
        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .get_fido_details()
                .await
                .map(|it| it.map(Fido2ResponseFfi::from))
                .map_err(LoginError::from)
        })
        .await
    }

    /// Submit 2FA fido2 code.
    pub async fn submit_fido(&self, fido_data: Fido2RequestFfi) -> Result<(), LoginError> {
        let flow = self.flow.clone();
        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .submit_fido(fido_data.into())
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
        let mut data = data;
        let flow = self.flow.clone();

        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;

            guard
                .migration_snooper()
                .run(
                    &data.user_id,
                    data.address_signature_enabled,
                    data.mobile_signature.take(),
                    data.mobile_signature_enabled,
                )
                .await
                .inspect_err(|err| warn!("{err:?}"))
                .map_err(|_| LoginError::Other("Couldn't process migration data".into()))?;

            let (user_id, session_id, user_data, refresh_token) = data.into_parts();

            guard
                .migrate(user_id.into(), session_id.into(), user_data, refresh_token)
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
        &self,
        need_encryption_key: bool,
    ) -> Result<String, LoginError> {
        let flow = self.flow.clone();
        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .generate_sign_in_qr_code(need_encryption_key)
                .await
                .map_err(LoginError::from)
        })
        .await
        .into()
    }

    /// Verifies host device confirmation for QR code login and completes the authentication process.
    ///
    /// This method waits for host device confirmation of the QR code login, decodes the payload using
    /// the provided encryption key, fetches user information, validates the passphrase, and stores user
    /// data. On success, it constructs a completed authentication state with session details.
    pub async fn check_host_device_confirmation(&self) -> Result<QrPollingResult, LoginError> {
        let flow = self.flow.clone();
        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .check_host_device_confirmation()
                .await
                .map_err(LoginError::from)?;
            if guard.is_awaiting_host_device_confirmation() {
                Ok(QrPollingResult::StillPolling)
            } else {
                Ok(QrPollingResult::Confirmed)
            }
        })
        .await
        .into()
    }

    /// Returns a password validator service.
    pub async fn password_validator(&self) -> Option<Arc<PasswordValidatorService>> {
        let flow = self.flow.clone();

        uniffi_async::<_, JoinError, _>(async move {
            Ok(Arc::new(PasswordValidatorService::setup(
                flow.lock().await.api().to_owned().into_dyn(),
            )))
        })
        .await
        .ok()
    }
}

#[derive(UniffiEnum)]
pub enum QrPollingResult {
    StillPolling,
    Confirmed,
}

#[uniffi_export]
impl LoginFlow {
    /// Check whether the login flow is waiting for Host Device confirmationo
    #[must_use]
    pub fn is_awaiting_host_device_confirmation(&self) -> bool {
        async_runtime().block_on(async {
            self.flow
                .lock()
                .await
                .is_awaiting_host_device_confirmation()
        })
    }

    /// Check whether the login flow has completed.
    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_logged_in() })
    }

    /// Return delinquent state of the user
    #[must_use]
    pub fn delinquent_state(&self) -> Result<DelinquentState, LoginError> {
        async_runtime().block_on(async {
            self.flow
                .lock()
                .await
                .delinquent_state()
                .map_err(LoginError::from)
                .map(DelinquentState::from)
        })
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

    /// Submit a new password for users with temporary passwords.
    pub async fn submit_new_password(&self, new_password: String) -> Result<(), LoginError> {
        let flow = self.flow.clone();
        uniffi_async::<_, LoginError, _>(async move {
            let mut guard = flow.lock().await;
            guard
                .submit_new_password(new_password)
                .await
                .map_err(LoginError::from)
        })
        .await
        .into()
    }

    /// Check whether the login flow is awaiting a new password.
    #[must_use]
    pub fn is_awaiting_new_password(&self) -> bool {
        async_runtime().block_on(async { self.flow.lock().await.is_awaiting_new_password() })
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

    /// Returned if the credentials are invalid.
    InvalidCredentials,

    /// Returned if Incorrect 2FA code was provided by the user
    Incorrect2FACode,

    /// Returned if the user key cannot be unlocked.
    CantUnlockUserKey,

    /// Returned if the user is forbidden from logging in.
    NoLogin,

    /// Returned if the user has no proton address.
    NoAddress,

    /// Returned if the initial auth request fails.
    FlowLogin(UserApiServiceError),

    /// Returned if the TOTP code submission fails.
    FlowTotp(UserApiServiceError),

    /// Returned if the FIDO2 challenge response fails.
    FlowFido(UserApiServiceError),

    /// Returned if the auth is missing from the store after login.
    MissingSession,

    /// Returned if a duplicate session is found in the store after login.
    DuplicateSession(String),

    /// Returned if we fail to fetch the user info after login.
    UserFetch(UserApiServiceError),

    /// Returned if we fail to fetch the user settings after login.
    SettingsFetch(UserApiServiceError),

    /// Returned if we fail to setup the user key.
    UserKeySetup(String),

    /// Returned if we decide not to setup the user key.
    UserKeySetupAborted,

    /// Returned if we fail to fetch the user's addresses after login.
    AddressFetch(UserApiServiceError),

    /// Returned if we fail to set up a new address.
    AddressSetup(String),

    /// Returned if we fail to set up a new address key.
    AddressKeySetup(String),

    /// Returned if we decide not to setup the address keys.
    AddressKeySetupAborted,

    /// TODO: Document this variant.
    KeySecretSaltFetch(UserApiServiceError),

    /// Authentication Store operation failed.
    AuthStore(String),

    /// Error during network call
    ApiError(String),

    /// Failed to poll the fork for completion
    WithCodePollFlowFailed(String),

    // Failed to encode QR login payload
    QRLoginEncoding,

    // Post login validation failed
    PostLoginValidationFailed(PostLoginValidationError),

    Other(String),
}

#[derive(Debug, UniffiEnum)]
pub enum PostLoginValidationError {
    /// Returned when login is aborted when the limit of free accounts is exceeded. Contains the max number of free accounts allowed.
    FreeAccountLimitExceeded(u64),
}

impl From<login_api::LoginError> for LoginError {
    fn from(value: login_api::LoginError) -> Self {
        match value {
            login_api::LoginError::InvalidState => LoginError::InvalidState,
            login_api::LoginError::NoLogin => Self::NoLogin,
            login_api::LoginError::NoAddress => Self::NoAddress,

            login_api::LoginError::FlowLogin(ApiServiceError::UnprocessableEntity(..))
            | login_api::LoginError::KeySecretSaltFetch(ApiServiceError::UnprocessableEntity(..))
            | login_api::LoginError::ServerProof(..)
            | login_api::LoginError::SrpProof(..) => LoginError::InvalidCredentials,

            login_api::LoginError::FlowTotp(ApiServiceError::UnprocessableEntity(..)) => {
                LoginError::Incorrect2FACode
            }

            login_api::LoginError::FlowLogin(e) => LoginError::FlowLogin(e.into()),
            login_api::LoginError::FlowTotp(e) => LoginError::FlowTotp(e.into()),
            login_api::LoginError::FlowFido(e) => LoginError::FlowFido(e.into()),

            login_api::LoginError::UserFetch(e) => LoginError::UserFetch(e.into()),
            login_api::LoginError::SettingsFetch(e) => LoginError::SettingsFetch(e.into()),
            login_api::LoginError::UserKeySetup(e) | login_api::LoginError::NewPasswordSetup(e) => {
                LoginError::UserKeySetup(e)
            }
            login_api::LoginError::UserKeySetupAborted
            | login_api::LoginError::NewPasswordSetupAborted => LoginError::UserKeySetupAborted,

            login_api::LoginError::AddressFetch(e) => LoginError::AddressFetch(e.into()),
            login_api::LoginError::AddressSetup(e) => LoginError::AddressSetup(e.clone()),
            login_api::LoginError::AddressKeySetup(e) => LoginError::AddressKeySetup(e.clone()),
            login_api::LoginError::AddressKeySetupAborted => LoginError::AddressKeySetupAborted,

            login_api::LoginError::MissingSession => LoginError::MissingSession,
            login_api::LoginError::DuplicateSession(id) => LoginError::DuplicateSession(id),

            login_api::LoginError::MissingPrimaryKey
            | login_api::LoginError::KeySecretDecryption
            | login_api::LoginError::KeySecretDerivation(_) => LoginError::CantUnlockUserKey,

            login_api::LoginError::KeySecretSaltFetch(e) => {
                LoginError::KeySecretSaltFetch(e.into())
            }

            login_api::LoginError::AuthStore(error) => LoginError::AuthStore(error.to_string()),
            login_api::LoginError::ApiError(e) => LoginError::ApiError(e.to_string()),
            login_api::LoginError::WithCodePollFlowFailed(e) => {
                LoginError::WithCodePollFlowFailed(e.to_string())
            }

            login_api::LoginError::QRLoginEncoding => Self::QRLoginEncoding,

            login_api::LoginError::PostLoginCheckFailed(
                RealPostLoginValidationError::FreeAccountLimitExceeded(limit),
            ) => Self::PostLoginValidationFailed(
                PostLoginValidationError::FreeAccountLimitExceeded(limit),
            ),
            login_api::LoginError::PostLoginCheckFailed(RealPostLoginValidationError::Other(
                error,
            )) => Self::Other(error.to_string()),
        }
    }
}

impl From<JoinError> for LoginError {
    fn from(value: JoinError) -> Self {
        Self::Other(value.to_string())
    }
}

/// Represents the delinquent state of the user.
///
/// This enum indicates the payment status of the user's account.
#[derive(UniffiEnum)]
pub enum DelinquentState {
    /// The user's account is fully paid.
    Paid = 0,
    /// The user's account is available but not yet paid.
    Available = 1,
    /// The user's account has an overdue payment.
    Overdue = 2,
    /// The user's account is delinquent due to unpaid dues.
    Delinquent = 3,
    /// The user's payment has not been received.
    NotReceived = 4,
}

impl From<responses_api::DelinquentState> for DelinquentState {
    fn from(value: responses_api::DelinquentState) -> Self {
        match value {
            responses_api::DelinquentState::Paid => DelinquentState::Paid,
            responses_api::DelinquentState::Available => DelinquentState::Available,
            responses_api::DelinquentState::Overdue => DelinquentState::Overdue,
            responses_api::DelinquentState::Delinquent => DelinquentState::Delinquent,
            responses_api::DelinquentState::NotReceived => DelinquentState::NotReceived,
        }
    }
}

#[derive(UniffiEnum)]
pub enum ProcessTargetDeviceQrError {
    ParseError(String),
    EncryptionFailed(String),
    Api(String),
    PassphraseAcquire(String),
    Other(String),
}

impl From<RealProcessTargetDeviceQrError> for ProcessTargetDeviceQrError {
    fn from(value: RealProcessTargetDeviceQrError) -> Self {
        match value {
            RealProcessTargetDeviceQrError::ParseError(err) => {
                ProcessTargetDeviceQrError::ParseError(err.to_string())
            }
            RealProcessTargetDeviceQrError::EncryptionFailed(err) => {
                ProcessTargetDeviceQrError::EncryptionFailed(err.to_string())
            }
            RealProcessTargetDeviceQrError::Api(err) => {
                ProcessTargetDeviceQrError::Api(err.to_string())
            }
            RealProcessTargetDeviceQrError::PassphraseAcquire(err) => {
                ProcessTargetDeviceQrError::PassphraseAcquire(err.to_string())
            }
        }
    }
}

impl From<JoinError> for ProcessTargetDeviceQrError {
    fn from(value: JoinError) -> Self {
        Self::Other(value.to_string())
    }
}
