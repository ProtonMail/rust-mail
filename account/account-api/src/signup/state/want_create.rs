#![allow(clippy::wildcard_imports)]
#![allow(clippy::similar_names)]

use crate::AccountApi;
use crate::requests::*;
use crate::responses::*;
use crate::shared::crypto::NewUserKey;
use crate::signup::ChallengeInfo;
use crate::signup::SignupError;
use crate::signup::state::Recovery;
use crate::signup::state::StateData;
use crate::signup::state::StateResult;
use crate::signup::state::Username;
use crate::signup::state::complete::Complete;
use derive_more::Display;
use futures::TryFutureExt;
use muon::Client;
use muon::client::flow::LoginExtraInfo;
use muon::client::flow::LoginFlow;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::SessionId;
use proton_core_api::services::proton::UserId;
use proton_core_api::store::AuthInfo;
use proton_core_api::store::DynStore;
use proton_core_api::store::TfaMode;
use proton_core_api::store::UserData;
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::salts::KeySecret;
use std::collections::HashMap;

/// Represents the state where the user can provide recovery information.
#[derive(Debug, Display, Clone)]
#[display("WantCreate")]
pub struct WantCreate {
    client: Client,
    username: Username,
    password: String,
    recovery: Recovery,
    data: StateData,
}

impl WantCreate {
    pub fn new(
        client: Client,
        username: Username,
        password: String,
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
        }
    }

    pub async fn create(self, store: DynStore) -> StateResult {
        let srp = new_srp_provider();
        let pgp = new_pgp_provider();

        let auth = self
            .auth_input(&srp)
            .inspect_err(|err| error!("auth_input: {err}"))
            .map_err(|_| SignupError::AccountCreationFailed)
            .await?;

        let payload = create_payload(&self.data.challenge_info);

        let user = self
            .create_user(&auth, payload)
            .inspect_err(|err| error!("create_user: {err}"))
            .map_err(|_| SignupError::AccountCreationFailed)
            .await?;

        store.write().await.set_name_or_addr(&user.email);

        let flow = (self.client.clone().auth())
            .login_with_extra(&user.email, &self.password, LoginExtraInfo::default())
            .await;

        let client = match flow {
            LoginFlow::Ok(client, data) => {
                info!("Login successful after signup");

                let info = AuthInfo {
                    user_id: UserId::from(user.id.clone()),
                    session_id: SessionId::from(data.session_id),
                    tfa_mode: TfaMode::none(),
                    mbp_mode: data.password_mode.into(),
                };

                store
                    .write()
                    .await
                    .set_auth_info(info)
                    .map_err(SignupError::SetAuthInfoFailed)
                    .await?;

                client
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
            key_secret: UserKeySecret(key_secret),
        };

        store
            .write()
            .await
            .set_user_data(data)
            .await
            .map_err(SignupError::SetUserDataFailed)?;

        Ok(Complete::new(client, user, addr).into())
    }

    async fn auth_input(&self, srp: &impl SRPProvider) -> Result<AuthInput, SignupError> {
        let res = (self.client)
            .get_auth_modulus()
            .inspect_err(|e| error!("get auth modulus: {e:?}"))
            .await?;

        let ver = srp
            .generate_client_verifier(&self.password, &res.modulus)
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
        payload: Option<HashMap<String, PayloadFrame>>,
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
                    .inspect_err(|e| error!("create internal user: {e} ({:?})", e.body_str()))
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
                    .inspect_err(|e| error!("create external user: {e} ({:?})", e.body_str()))
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
        let user_key = NewUserKey::init(srp, pgp, &self.password)?;
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

fn create_payload(challenge_info: &ChallengeInfo) -> Option<HashMap<String, PayloadFrame>> {
    if challenge_info.recovery_behavior.is_none() && challenge_info.username_behavior.is_none() {
        return None;
    }

    let mut payload = HashMap::with_capacity(2);

    if let Some(behavior) = challenge_info.recovery_behavior.clone() {
        insert_payload_frame(
            &mut payload,
            PayloadFrameMetadata::Recovery,
            challenge_info,
            PayloadFrameBehavior::Recovery(behavior),
        );
    }

    if let Some(behavior) = challenge_info.username_behavior.clone() {
        insert_payload_frame(
            &mut payload,
            PayloadFrameMetadata::Username,
            challenge_info,
            PayloadFrameBehavior::Username(behavior),
        );
    }

    Some(payload)
}

fn insert_payload_frame(
    payload: &mut HashMap<String, PayloadFrame>,
    metadata: PayloadFrameMetadata,
    challenge_info: &ChallengeInfo,
    behavior: PayloadFrameBehavior,
) {
    let id = payload.len();
    let name = format!("{}-challenge-{id}", challenge_info.product_version);
    payload.insert(
        name,
        PayloadFrameV2_2 {
            metadata,
            device_info: challenge_info.device_info.clone(),
            user_behavior: Some(behavior),
        }
        .into(),
    );
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use proton_core_common::device::DeviceInfo;

    use crate::{
        prelude::{Behavior, PayloadFrameBehavior, PayloadFrameMetadata, PayloadFrameV2_2},
        signup::{ChallengeInfo, state::want_create::create_payload},
    };

    #[test]
    fn test_create_payload() {
        let device_info = DeviceInfo {
            language: "lang".into(),
            timezone: "tz".into(),
            timezone_offset: -60,
            model: "model".into(),
            brand: "brand".into(),
            codename: "code".into(),
            uuid: "uuid".into(),
            country: "country".into(),
            rooted: false,
            font_scale: "2.0".into(),
            storage: 42.0,
            dark_mode: true,
            keyboards: vec!["kb_1".into()],
        };
        let username_behavior = Behavior {
            time: vec![123],
            click: 12,
            copy: vec!["usr_cf".into()],
            paste: vec!["usr_pf".into()],
            keydown: vec!["usr_kdf".into()],
        };
        let recovery_behavior = Behavior {
            time: vec![456],
            click: 34,
            copy: vec!["rec_cf".into()],
            paste: vec!["rec_pf".into()],
            keydown: vec!["rec_kdf".into()],
        };
        let challenge_info = ChallengeInfo {
            product_version: "mail-v1".into(),
            device_info: Some(device_info.clone()),
            recovery_behavior: Some(recovery_behavior.clone()),
            username_behavior: Some(username_behavior.clone()),
        };
        let payload = create_payload(&challenge_info);
        assert_eq!(
            payload,
            Some(HashMap::from_iter([
                (
                    "mail-v1-challenge-0".to_string(),
                    PayloadFrameV2_2 {
                        metadata: PayloadFrameMetadata::Recovery,
                        device_info: Some(device_info.clone()),
                        user_behavior: Some(PayloadFrameBehavior::Recovery(recovery_behavior)),
                    }
                    .into(),
                ),
                (
                    "mail-v1-challenge-1".to_string(),
                    PayloadFrameV2_2 {
                        metadata: PayloadFrameMetadata::Username,
                        device_info: Some(device_info.clone()),
                        user_behavior: Some(PayloadFrameBehavior::Username(username_behavior)),
                    }
                    .into(),
                )
            ]))
        );
    }
}
