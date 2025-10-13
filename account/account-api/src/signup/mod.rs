use crate::countries::{COUNTRIES, Country};
use crate::prelude::{Address, User};
use crate::shared::SecureString;
use crate::shared::challenge::{Behavior, ChallengeInfo};
use crate::shared::crypto::SharedCryptoError;
use crate::signup::state::{Recovery, StateKind, Username};
use crate::{AccountApi, ApiError};
use futures::TryFutureExt;
use proton_core_api::services::observability::{
    ApiServiceObservabilityResponse, ObservabilityRecorder,
};
use proton_core_api::store::{DynStore, StoreError};
use proton_core_api::{metric, services::observability::ObservabilityMetric};
use proton_core_common::post_login_check::{PostLoginValidationError, PostLoginValidator};
use proton_crypto_account::errors::{AccountCryptoError, SKLError};
use proton_crypto_account::{proton_crypto::CryptoError, salts::SaltError};
use serde::{Deserialize, Serialize};
use state::State;
use std::fmt::Debug;
use thiserror::Error;

pub mod state;

/// Errors that can occur during the signup flow.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum SignupError {
    /// An underlying API call failed.
    Api(#[from] ApiError),

    /// A crypto error occurred.
    Crypto(#[from] SignupCryptoError),

    /// Signup is blocked (e.g., anti-abuse).
    #[error("Signup blocked by server")]
    SignupBlockedByServer,

    /// Username is unavailable.
    #[error("Username unavailable")]
    UsernameUnavailable(Option<String>),

    /// Placeholder for errors during account creation step.
    #[error("Account creation failed")]
    AccountCreationFailed,

    /// Placeholder for errors during address setup step.
    #[error("Address setup failed")]
    AddressSetupFailed,

    /// Placeholder for errors during key setup step.
    #[error("Key setup failed")]
    KeySetupFailed,

    /// The auth info could not be set in the store.
    #[error("SetAuthInfo failed: {0}")]
    SetAuthInfoFailed(StoreError),

    /// The user data could not be set in the store.
    #[error("SetUserData failed: {0}")]
    SetUserDataFailed(StoreError),

    /// The requested operation is not valid in the current state of the flow.
    #[error("Operation is not valid in the current state")]
    InvalidState,

    /// The recovery email format is invalid
    #[error("Recovery email format is invalid")]
    RecoveryEmailInvalid,

    /// The recovery phone number format is invalid
    #[error("Recovery phone number format is invalid")]
    RecoveryPhoneNumberInvalid,

    #[error("Post-login check failed: {0}")]
    PostLoginCheckFailed(#[from] PostLoginValidationError),
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct SignupCryptoError(String);

impl From<SaltError> for SignupError {
    fn from(e: SaltError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<CryptoError> for SignupError {
    fn from(e: CryptoError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<AccountCryptoError> for SignupError {
    fn from(e: AccountCryptoError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<SKLError> for SignupError {
    fn from(e: SKLError) -> Self {
        Self::Crypto(SignupCryptoError(e.to_string()))
    }
}

impl From<SharedCryptoError> for SignupError {
    fn from(e: SharedCryptoError) -> Self {
        match e {
            SharedCryptoError::Salt(e) => e.into(),
            SharedCryptoError::Crypto(e) => e.into(),
            SharedCryptoError::AccountCrypto(e) => e.into(),
            SharedCryptoError::SKL(e) => e.into(),
        }
    }
}

/// A signup flow that can be used to sign up a user.
///
/// The flow guides the user through the signup process, ensuring all necessary steps
/// are completed in the correct order.
pub struct SignupFlow {
    client: muon::Client,
    store: DynStore,
    state: Vec<State>,
    domains: Vec<String>,
    countries: Vec<Country>,
    post_login_validator: Box<dyn PostLoginValidator>,
}

impl SignupFlow {
    /// Create a new signup flow, implicitly fetching available domains.
    pub async fn new(
        client: muon::Client,
        store: DynStore,
        challenge_info: ChallengeInfo,
        post_login_validator: Box<dyn PostLoginValidator>,
    ) -> Result<Self, ApiError> {
        let recorder = ObservabilityRecorder::default();

        let domains = client
            .get_available_domains(Some("signup".to_owned()))
            .inspect_ok(|_| {
                recorder.record(DomainAvailability::success());
            })
            .inspect_err(|err| {
                recorder.record(DomainAvailability::error(err));
            })
            .await?
            .domains;

        let countries = COUNTRIES.to_owned();
        let state = vec![State::new(client.clone(), challenge_info)];

        Ok(Self {
            client,
            store,
            state,
            domains,
            countries,
            post_login_validator,
        })
    }

    /// Get available domains.
    #[must_use]
    pub fn available_domains(&self) -> &[String] {
        &self.domains
    }

    /// Get available countries.
    #[must_use]
    pub fn available_countries(&self) -> &[Country] {
        &self.countries
    }

    /// Get the kind of the current state.
    pub fn kind(&self) -> Result<StateKind, SignupError> {
        Ok(self.state()?.kind())
    }

    /// Submit Proton Internal Username
    pub async fn submit_internal_username(
        &mut self,
        username: String,
        domain: String,
        behavior: Option<Behavior>,
    ) -> Result<(), SignupError> {
        let username = Username::Internal { username, domain };

        let next = self.state()?.submit_username(username, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Submit Proton External email
    pub async fn submit_external_username(
        &mut self,
        email: String,
        behavior: Option<Behavior>,
    ) -> Result<(), SignupError> {
        let username = Username::External { email };

        let next = self.state()?.submit_username(username, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Submit password.
    pub fn submit_password(
        &mut self,
        password: impl Into<SecureString>,
    ) -> Result<(), SignupError> {
        let next = self.state()?.submit_password(password.into())?;

        self.state.push(next);

        Ok(())
    }

    /// Submit a recovery email.
    pub async fn submit_recovery_email(
        &mut self,
        email: String,
        behavior: Option<Behavior>,
    ) -> Result<(), SignupError> {
        let recovery = Recovery::Email(email);

        let next = self.state()?.submit_recovery(recovery, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Submit a recovery phone number.
    pub async fn submit_recovery_phone(
        &mut self,
        phone: String,
        behavior: Option<Behavior>,
    ) -> Result<(), SignupError> {
        let recovery = Recovery::Phone(phone);

        let next = self.state()?.submit_recovery(recovery, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Skip recovery information.
    pub async fn skip_recovery(&mut self, behavior: Option<Behavior>) -> Result<(), SignupError> {
        let recovery = Recovery::None;

        let next = self.state()?.submit_recovery(recovery, behavior).await?;

        self.state.push(next);

        Ok(())
    }

    /// Create the account.
    pub async fn create(&mut self) -> Result<(), SignupError> {
        let store = DynStore::clone(&self.store);

        let next = self
            .state()?
            .create(store, &*self.post_login_validator)
            .await?;

        self.state.push(next);

        Ok(())
    }

    /// Complete the signup flow.
    pub fn complete(&mut self) -> Result<(muon::Client, User, Address), SignupError> {
        self.state()?.complete()
    }

    /// Return to the previous state.
    pub fn back(&mut self) -> Result<(), SignupError> {
        if self.state.len() < 2 {
            return Err(SignupError::InvalidState);
        }

        self.state.pop();

        Ok(())
    }

    #[must_use]
    pub fn api(&self) -> &muon::Client {
        &self.client
    }

    fn state(&self) -> Result<State, SignupError> {
        self.state.last().cloned().ok_or(SignupError::InvalidState)
    }
}

metric! {
    #[name = "core_signup_fetch_domains_total"]
    #[version = 1]
    #[doc = "Records the outcomes of the `GET core/v4/domains/available` API calls on the origin device."]
    pub struct DomainAvailability {
        pub status: ApiServiceObservabilityResponse,
    }
}

impl DomainAvailability {
    fn success() -> Self {
        DomainAvailability {
            status: ApiServiceObservabilityResponse::Success,
        }
    }
    fn error(error: &ApiError) -> Self {
        DomainAvailability {
            status: error.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_api::services::{
        observability::ObservabilityRecorder,
        proton::prelude::{PostMetricsRequestData, PostMetricsRequestElement},
    };
    use serde_json::{self, json};

    fn assert_serialization_deserialization(
        status: ApiServiceObservabilityResponse,
        expected_status: &str,
    ) {
        let metric = ObservabilityRecorder::into_metrics_element(
            DomainAvailability { status },
            1_741_021_308,
            1,
        )
        .unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();

        let expected_json = format!(
            r#"{{"Name":"core_signup_fetch_domains_total","Version":1,"Timestamp":1741021308,"Data":{{"Labels":{{"status":"{expected_status}"}},"Value":1}}}}"#
        );

        assert_eq!(serialized, expected_json);

        assert_eq!(
            PostMetricsRequestElement {
                name: "core_signup_fetch_domains_total".into(),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({
                        "status": expected_status,
                    }),
                    value: 1,
                }
            },
            serde_json::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_fetch_domains_serialization_deserialization_for_all_variants() {
        let statuses = vec![
            (ApiServiceObservabilityResponse::Success, "success"),
            (ApiServiceObservabilityResponse::Http4xx, "http4xx"),
            (ApiServiceObservabilityResponse::Http5xx, "http5xx"),
            (
                ApiServiceObservabilityResponse::NetworkError,
                "network_error",
            ),
            (
                ApiServiceObservabilityResponse::SerializationError,
                "serialization_error",
            ),
            (ApiServiceObservabilityResponse::Unknown, "unknown"),
        ];

        for (status, expected_status) in statuses {
            assert_serialization_deserialization(status, expected_status);
        }
    }
}
