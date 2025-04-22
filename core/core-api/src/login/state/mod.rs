use crate::auth::UserKeySecret;
use crate::login::LoginError;
use crate::login::state::complete::Complete;
use crate::login::state::want_login::WantLogin;
use crate::login::state::want_mbp::WantMbp;
use crate::login::state::want_tfa::{TfaFlow, WantTfa};
use crate::services::observability::{
    ApiServiceObservabilityResponse, ObservabilityRecorder, metrics,
};
use crate::services::proton::Proton;
use crate::services::proton::ProtonCore;
use crate::services::proton::{SessionId, UserId};
use crate::session::{Session, SessionParts};
use crate::store::UserData;
use derive_more::{Debug, From};
use futures::TryFutureExt;
use muon::client::flow::{AuthFlow, LoginExtraInfo, LoginFlowData};
use proton_crypto_account::keys::{LockedKey, UserKeys};
use proton_crypto_account::proton_crypto;
use proton_crypto_account::salts::{Salt, Salts};
use secrecy::SecretString;

mod complete;
mod want_login;
mod want_mbp;
mod want_tfa;

/// Represents the possible states that the login flow can be in,
/// ensuring only valid transitions between states are possible.
#[derive(Debug, From)]
pub enum State {
    /// The flow is waiting for the user to provide their login credentials.
    #[debug("WantLogin")]
    WantLogin(WantLogin),

    /// A recoverable error occurred during the `WantLogin` state.
    #[debug("LoginRetry")]
    LoginRetry,

    /// The flow is waiting for the user to provide a 2FA token.
    #[debug("WantTfa")]
    WantTfa(WantTfa),

    /// A recoverable error occurred during the `WantTfa` state.
    #[debug("TfaRetry")]
    TfaRetry(UserId, SessionId, Option<String>),

    /// An error occurred during the `WantTfa` state.
    #[debug("TfaError")]
    TfaError,

    /// The flow is waiting for the user to provide their mailbox password.
    #[debug("WantMbp")]
    WantMbp(WantMbp),

    /// A recoverable error occurred during the `WantMbp` state.
    #[debug("MbpRetry")]
    MbpRetry(UserId, SessionId),

    /// The flow is complete.
    #[debug("Complete")]
    Complete(Complete),

    /// Invalid state, cannot be used.
    #[debug("Invalid")]
    Invalid,
}

/// Public actions that can be taken on the state.
impl State {
    /// Attempt to login with the provided credentials.
    pub async fn login(
        self,
        user: String,
        pass: String,
        info: LoginExtraInfo,
    ) -> Result<Self, (Self, LoginError)> {
        if let Self::WantLogin(state) = self {
            Ok(state.login(user, pass, info).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to migrate existing alive session from
    /// the Legacy version of the application.
    pub async fn migrate(
        self,
        client: Proton,
        user: UserData,
        data: LoginFlowData,
        refresh_token: SecretString,
    ) -> Result<Self, (Self, LoginError)> {
        let Self::WantLogin(state) = self else {
            return Err((self, LoginError::InvalidState));
        };

        state.migrate(client, user, data, refresh_token).await
    }

    /// Attempt to submit a TOTP code.
    pub async fn submit_totp(self, code: String) -> Result<Self, (Self, LoginError)> {
        if let Self::WantTfa(state) = self {
            Ok(state.submit_totp(code).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to submit a FIDO code.
    #[allow(unused)]
    pub async fn submit_fido(self, code: String) -> Result<Self, (Self, LoginError)> {
        if let Self::WantTfa(state) = self {
            Ok(state.submit_fido(code).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to submit a mailbox password.
    pub async fn submit_mbp(self, pass: String) -> Result<Self, (Self, LoginError)> {
        if let Self::WantMbp(state) = self {
            Ok(state.submit_mbp(pass).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to take the completed session from the flow.
    pub fn into_session(self) -> Result<Session, LoginError> {
        if let Self::Complete(state) = self {
            Ok(state.into_session())
        } else {
            Err(LoginError::InvalidState)
        }
    }

    /// Get the user ID of the user that has (or is in the process of) logging in.
    pub fn user_id(&self) -> Result<&UserId, LoginError> {
        let state: &dyn HasUserId = match self {
            Self::WantTfa(state) => state,
            Self::WantMbp(state) => state,
            Self::Complete(state) => state,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state.user_id())
    }

    /// Get the session ID that has been (or is in the process of) being created.
    pub fn session_id(&self) -> Result<&SessionId, LoginError> {
        let state: &dyn HasSessionId = match self {
            Self::WantTfa(state) => state,
            Self::WantMbp(state) => state,
            Self::Complete(state) => state,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state.session_id())
    }
}

/// Public entrypoints for creating new states.
impl State {
    /// Create a `WantLogin` state.
    pub fn new(client: Proton, parts: SessionParts) -> Self {
        Self::want_login(client.auth(), parts)
    }

    /// Create a `WantTfa` state from a resumed login flow.
    pub fn new_from_tfa(
        client: Proton,
        parts: SessionParts,
        user_id: UserId,
        session_id: SessionId,
        pass: Option<String>,
    ) -> Self {
        let data = StateData {
            parts,
            user_id,
            session_id,
            observability: ObservabilityRecorder::default(),
        };

        Self::want_tfa(client.auth().into(), data, pass)
    }

    /// Create a `WantMbp` state from a resumed login flow.
    pub fn new_from_mbp(
        client: Proton,
        parts: SessionParts,
        user_id: UserId,
        session_id: SessionId,
    ) -> Self {
        let data = StateData {
            parts,
            user_id,
            session_id,
            observability: ObservabilityRecorder::default(),
        };

        Self::want_mbp(client, data)
    }
}

/// Private entrypoints for creating new states.
impl State {
    /// Create a `WantLogin` state.
    fn want_login(auth: AuthFlow, parts: SessionParts) -> Self {
        WantLogin::new(auth, parts).into()
    }

    /// Create a `WantTfa` state.
    fn want_tfa(flow: TfaFlow, data: StateData, pass: Option<String>) -> Self {
        WantTfa::new(flow, data, pass).into()
    }

    /// Create a `WantMbp` state.
    fn want_mbp(client: Proton, data: StateData) -> Self {
        WantMbp::new(client, data).into()
    }

    /// Finalize login flow for the migration.
    async fn finalize_migration(
        client: Proton,
        data: StateData,
        user_data: UserData,
    ) -> Result<Self, LoginError> {
        data.parts
            .store
            .write()
            .await
            .set_user_data(user_data)
            .await?;

        Ok(Complete::new(client, data).into())
    }

    /// Attempt to finalize the login flow, transitioning to the `Complete` state if successful.
    async fn finalize(client: Proton, data: StateData, pass: String) -> Result<Self, LoginError> {
        // Initialize the crypto providers.
        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        // Fetch user info to trigger HV.
        let user = client
            .get_users()
            .map_ok(|res| res.user)
            .inspect_err(|err| {
                data.observability
                    .record(metrics::SignInSubmitMailBoxPwTotal::new(
                        metrics::MailboxPasswordMetricStatus::ApiService(err.into()),
                    ));
            })
            .map_err(LoginError::UserFetch)
            .await?;

        // Fetch the user's key salts.
        let salts = client
            .get_keys_salts()
            .map_ok(|res| res.key_salts)
            .inspect_err(|err| {
                data.observability
                    .record(metrics::SignInSubmitMailBoxPwTotal::new(
                        metrics::MailboxPasswordMetricStatus::ApiService(err.into()),
                    ));
            })
            .map_err(LoginError::KeySecretSaltFetch)
            .await?;

        // Build the salts object.
        let salts = Salts::new(salts.into_iter().map(|salt| Salt {
            id: salt.id.into_inner().into(),
            key_salt: salt.key_salt.map(Into::into),
        }));

        // Derive the key secret to unlock the user keys.
        let secret = if let Some(key) = user.keys.primary() {
            (salts.salt_for_key(&srp, &key.id, pass.as_bytes()))
                .inspect_err(|_| {
                    data.observability
                        .record(metrics::SignInSubmitMailBoxPwTotal::new(
                            metrics::MailboxPasswordMetricStatus::KeyDerivationFailed,
                        ));
                })
                .map_err(LoginError::KeySecretDerivation)?
        } else {
            return Err(LoginError::MissingPrimaryKey);
        };

        // Check if the key secret can unlock the user keys.
        let secret = if user.keys.unlock(&pgp, &secret).unlocked_keys.is_empty() {
            data.observability
                .record(metrics::SignInSubmitMailBoxPwTotal::new(
                    metrics::MailboxPasswordMetricStatus::KeyUnlockFailed,
                ));
            return Err(LoginError::KeySecretDecryption);
        } else {
            UserKeySecret(secret)
        };

        // Save the derived user data in the auth store.
        (data.parts.store.write().await)
            .set_user_data(UserData {
                username: user.name.unwrap_or_default(),
                display_name: user.display_name.unwrap_or_default(),
                primary_addr: user.email,
                key_secret: secret,
            })
            .await?;

        data.observability
            .record(metrics::SignInSubmitMailBoxPwTotal::new(
                metrics::MailboxPasswordMetricStatus::ApiService(
                    ApiServiceObservabilityResponse::Success,
                ),
            ));
        Ok(Complete::new(client, data).into())
    }
}

pub(crate) struct StateData {
    parts: SessionParts,
    user_id: UserId,
    session_id: SessionId,
    observability: ObservabilityRecorder,
}

/// A trait for states in which the user ID is known.
trait HasUserId {
    fn user_id(&self) -> &UserId;
}

/// A trait for states in which the auth ID is known.
trait HasSessionId {
    fn session_id(&self) -> &SessionId;
}

/// A helper trait for working with user keys.
trait UserKeysExt {
    fn primary(&self) -> Option<&LockedKey>;
}

impl UserKeysExt for UserKeys {
    fn primary(&self) -> Option<&LockedKey> {
        self.as_ref().iter().find(|&key| key.primary)
    }
}
