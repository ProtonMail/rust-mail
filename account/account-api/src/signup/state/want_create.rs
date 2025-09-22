#![allow(clippy::wildcard_imports)]
#![allow(clippy::similar_names)]

use crate::requests::*;
use crate::responses::*;
use crate::shared::SecureString;
use crate::shared::challenge::ChallengePayload;
use crate::shared::crypto::NewUserKey;
use crate::signup::SignupError;
use crate::signup::state::Recovery;
use crate::signup::state::StateData;
use crate::signup::state::StateResult;
use crate::signup::state::Username;
use crate::signup::state::complete::Complete;
use crate::{AccountApi, ApiError};
use derive_more::Display;
use futures::TryFutureExt;
use muon::Client;
#[allow(deprecated)]
use muon::client::flow::LoginExtraInfo;
use muon::client::flow::LoginFlow;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::observability::ApiServiceObservabilityResponse;
use proton_core_api::services::proton::SessionId;
use proton_core_api::store::AuthInfo;
use proton_core_api::store::DynStore;
use proton_core_api::store::TfaMode;
use proton_core_api::store::UserData;
use proton_core_common::observability::PreLoginMetricRecorder;
use proton_core_common::post_login_check::PostLoginValidator;
use proton_core_common::post_login_check::UserCheckResult;
use proton_core_common::post_login_check::UserCheckStatus;
use proton_core_common::{metric, observability::ObservabilityMetric};
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::salts::KeySecret;
use serde::{Deserialize, Serialize};

/// Represents the state where the user can provide recovery information.
#[derive(Debug, Display, Clone)]
#[display("WantCreate")]
pub struct WantCreate {
    client: Client,
    username: Username,
    password: SecureString,
    recovery: Recovery,
    data: StateData,
    recorder: PreLoginMetricRecorder,
}

impl WantCreate {
    pub fn new(
        client: Client,
        username: Username,
        password: SecureString,
        recovery: Recovery,
        data: StateData,
    ) -> Self {
        info!("Signup flow wants create");

        Self {
            client,
            username,
            password,
            recovery,
            data,
            recorder: PreLoginMetricRecorder::default(),
        }
    }

    #[allow(deprecated)]
    pub async fn create(
        self,
        store: DynStore,
        post_login_validator: &dyn PostLoginValidator,
    ) -> StateResult {
        let srp = new_srp_provider();
        let pgp = new_pgp_provider();

        let auth = self
            .auth_input(&srp)
            .inspect_err(|err| error!("auth_input: {err}"))
            .map_err(|_| SignupError::AccountCreationFailed)
            .await?;

        let payload = ChallengePayload::new(&self.data.challenge_info);

        let user = self
            .create_user(&auth, payload)
            .inspect_err(|err| error!("create_user: {err}"))
            .map_err(|_| SignupError::AccountCreationFailed)
            .await?;

        store.write().await.set_name_or_addr(&user.email);

        let flow = (self.client.clone().auth())
            .login_with_extra(
                &user.email,
                self.password.as_str(),
                LoginExtraInfo::default(),
            )
            .await;

        let (client, password_mode) = match flow {
            LoginFlow::Ok(client, data) => {
                info!("Login successful after signup");

                let info = AuthInfo {
                    user_id: user.id.clone(),
                    session_id: SessionId::from(data.session_id),
                    tfa_mode: TfaMode::none(),
                };

                store
                    .write()
                    .await
                    .set_auth_info(info)
                    .map_err(SignupError::SetAuthInfoFailed)
                    .await?;

                (client, data.password_mode)
            }

            LoginFlow::TwoFactor(..) => {
                error!("Login failed after signup (2FA required)");
                return Err(SignupError::InvalidState);
            }

            LoginFlow::Failed { reason, .. } => {
                error!("Login failed after signup: {reason}");
                return Err(SignupError::InvalidState);
            }
        };

        let addr = client
            .get_addresses()
            .inspect_err(|err| error!("get_addresses: {err}"))
            .await?
            .addresses
            .into_iter()
            .next()
            .ok_or(SignupError::AddressSetupFailed)?;

        let (user, key_secret) = self
            .create_keys(&srp, &pgp, &client, &auth, &addr)
            .inspect_err(|err| error!("create_keys: {err}"))
            .map_err(|_| SignupError::KeySetupFailed)
            .await?;

        let data = UserData {
            username: user.name.clone().unwrap_or_default(),
            display_name: user.display_name.clone().unwrap_or_default(),
            primary_addr: addr.email.clone(),
            password_mode: password_mode.into(),
            key_secret: UserKeySecret(key_secret),
        };

        store
            .write()
            .await
            .set_user_data(data)
            .await
            .map_err(SignupError::SetUserDataFailed)?;

        let recorder = PreLoginMetricRecorder::default();
        // Validations are run after `set_user_data` is called, se even if the login flow is stopped and login is prevented for now,
        // the account itself remains in a "ready to use" state (e.g. is_ready flag is set) for later, when login rules are not violated anymore (e.g. logged-in free account count)
        match post_login_validator.validate(&user.clone().into()).await {
            Ok(()) => {
                recorder.record(UserCheckResult::new(UserCheckStatus::Success));
            }
            Err(err) => {
                recorder.record(UserCheckResult::new(UserCheckStatus::Failure));
                return Err(err.into());
            }
        }

        Ok(Complete::new(client, user, addr).into())
    }

    async fn auth_input(&self, srp: &impl SRPProvider) -> Result<AuthInput, SignupError> {
        let res = (self.client)
            .get_auth_modulus()
            .inspect_err(|e| error!("get auth modulus: {e:?}"))
            .await?;

        let ver = srp
            .generate_client_verifier(self.password.as_str(), &res.modulus)
            .inspect_err(|e| error!("generate client verifier: {e:?}"))?;

        Ok(AuthInput {
            version: ver.version,
            modulus_id: res.modulus_id,
            salt: ver.salt,
            verifier: ver.verifier,
        })
    }

    async fn create_user(
        &self,
        auth: &AuthInput,
        payload: Option<ChallengePayload>,
    ) -> Result<User, SignupError> {
        let (email, phone) = match &self.recovery {
            Recovery::Email(email) => (Some(email), None),
            Recovery::Phone(phone) => (None, Some(phone)),
            Recovery::None => (None, None),
        };

        let res = match &self.username {
            Username::Internal { username, domain } => {
                let req = CreateUserRequest {
                    user_type: CreateUserType::Normal,
                    username: username.to_owned(),
                    domain: Some(domain.to_owned()),
                    auth: auth.to_owned(),
                    email: email.cloned(),
                    phone: phone.cloned(),
                    referrer: None,
                    payload,
                };

                self.client
                    .create_user(req)
                    .inspect_err(|err| {
                        self.recorder
                            .record(UserStatus::error(UserKind::Internal, err));
                    })
                    .inspect_ok(|_| {
                        self.recorder
                            .record(UserStatus::success(UserKind::Internal));
                    })
                    .await?
            }

            Username::External { email } => {
                let req = CreateExternalUserRequest {
                    user_type: CreateUserType::Normal,
                    email: email.to_owned(),
                    auth: auth.to_owned(),
                    referrer: None,
                };

                self.client
                    .create_external_user(req)
                    .inspect_err(|err| {
                        self.recorder
                            .record(UserStatus::error(UserKind::External, err));
                    })
                    .inspect_ok(|_| {
                        self.recorder
                            .record(UserStatus::success(UserKind::External));
                    })
                    .await?
            }
        };

        Ok(res.user)
    }

    async fn create_keys(
        &self,
        srp: &impl SRPProvider,
        pgp: &impl PGPProviderSync,
        client: &Client,
        auth: &AuthInput,
        addr: &Address,
    ) -> Result<(User, KeySecret), SignupError> {
        let user_key = NewUserKey::init(srp, pgp, self.password.as_str())?;
        let addr_key = user_key.init_addr(pgp, &addr.email)?;

        let req = SetupKeysRequest {
            auth: auth.to_owned(),
            primary_key: user_key.key.private_key.to_string(),
            key_salt: user_key.salt.to_string(),
            address_keys: vec![AddressKeyInput::new(&addr.id, &addr_key.key, &addr_key.skl)],
            encrypted_secret: None,
            org_primary_user_key: None,
            org_activation_token: None,
        };

        let res = client
            .setup_keys(AsyncUserInitialization::CalledByClient, req)
            .await?;

        Ok((res.user, user_key.pass))
    }
}

#[derive(Display, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserKind {
    Internal,
    External,
}

metric! {
    #[name = "core_signup_account_creation_total"]
    #[version = 1]
    #[doc = "Records the outcomes of the `GET core/v4/users` and `GET core/v4/users/external` API calls on the origin device."]
    pub struct UserStatus {
        pub status: ApiServiceObservabilityResponse,
        pub kind: UserKind,
    }
}

impl UserStatus {
    fn success(kind: UserKind) -> Self {
        UserStatus {
            status: ApiServiceObservabilityResponse::Success,
            kind,
        }
    }
    fn error(kind: UserKind, error: &ApiError) -> Self {
        UserStatus {
            status: error.into(),
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proton_core_api::services::proton::prelude::{
        PostMetricsRequestData, PostMetricsRequestElement,
    };
    use proton_core_common::observability::into_metrics_element;
    use serde_json::{self, json};

    fn assert_serialization_deserialization(
        status: ApiServiceObservabilityResponse,
        expected_status: &str,
        kind: UserKind,
        expected_kind: &str,
    ) {
        let metric = into_metrics_element(UserStatus { status, kind }, 1_741_021_308, 1).unwrap();

        let serialized = serde_json::to_string(&metric).unwrap();

        let expected_json = format!(
            r#"{{"Name":"core_signup_account_creation_total","Version":1,"Timestamp":1741021308,"Data":{{"Labels":{{"kind":"{expected_kind}","status":"{expected_status}"}},"Value":1}}}}"#
        );

        assert_eq!(serialized, expected_json);

        assert_eq!(
            PostMetricsRequestElement {
                name: "core_signup_account_creation_total".into(),
                version: 1,
                timestamp: 1_741_021_308,
                data: PostMetricsRequestData {
                    labels: json!({
                        "status": expected_status,
                        "kind": expected_kind
                    }),
                    value: 1,
                }
            },
            serde_json::from_str(&serialized).unwrap()
        );
    }

    #[test]
    fn test_account_creation_result_serialization_deserialization_for_all_variants() {
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
            assert_serialization_deserialization(
                status,
                expected_status,
                UserKind::Internal,
                "internal",
            );
        }
    }
}
