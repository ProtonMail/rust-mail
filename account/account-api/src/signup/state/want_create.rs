#![allow(clippy::wildcard_imports)]
#![allow(clippy::similar_names)]

use crate::AccountApi;
use crate::requests::*;
use crate::responses::*;
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
use proton_crypto_account::keys::KeyFlag;
use proton_crypto_account::keys::KeyId;
use proton_crypto_account::keys::LocalAddressKey;
use proton_crypto_account::keys::LocalSignedKeyList;
use proton_crypto_account::keys::LocalUserKey;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_account::proton_crypto::crypto::KeyGeneratorAlgorithm;
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::salts::KeySalt;
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

    pub async fn create(
        self,
        store: DynStore,
        username_behavior: Option<UsernameBehavior>,
    ) -> StateResult {
        let srp = new_srp_provider();
        let pgp = new_pgp_provider();

        let auth = self
            .auth_input(&srp)
            .inspect_err(|err| error!("auth_input: {err}"))
            .map_err(|_| SignupError::AccountCreationFailed)
            .await?;

        let payload = create_payload(&self.data.challenge_info, username_behavior);

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
                    tfa_mode: TfaMode {
                        totp: false,
                        fido: false,
                    },
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
            .create_keys(&srp, &pgp, &client, &auth, &user, &addr)
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
        user: &User,
        addr: &Address,
    ) -> Result<(User, KeySecret), SignupError> {
        let key_salt = KeySalt::generate();
        let key_algo = KeyGeneratorAlgorithm::default();
        let key_pass = key_salt.salted_key_passphrase(srp, self.password.as_bytes())?;

        let user_key = LocalUserKey::generate(pgp, key_algo, &key_pass)?;
        let addr_key = create_addr_key(pgp, key_algo, &user.email, &user_key, &key_pass)?;
        let addr_skl = create_addr_skl(pgp, &user_key, &addr_key, &key_pass)?;

        let req = SetupKeysRequest {
            auth: auth.to_owned(),
            primary_key: user_key.private_key.to_string(),
            key_salt: key_salt.to_string(),
            address_keys: vec![build_address_key(addr, &addr_key, &addr_skl)],

            encrypted_secret: None,
            org_primary_user_key: None,
            org_activation_token: None,
        };

        let res = client
            .setup_keys_for_new_account(AsyncUserInitialization::CalledByClient, req)
            .await;

        Ok((res?.user, key_pass))
    }
}

fn create_addr_key(
    pgp: &impl PGPProviderSync,
    alg: KeyGeneratorAlgorithm,
    email: &str,
    user_key: &LocalUserKey,
    key_pass: &KeySecret,
) -> Result<LocalAddressKey, SignupError> {
    let key_id = new_key_id();
    let user_key = user_key.unlock_and_assign_key_id(pgp, key_id, key_pass)?;
    let addr_key = LocalAddressKey::generate(pgp, email, alg, KeyFlag::default(), true, &user_key)?;

    Ok(addr_key)
}

fn create_addr_skl(
    pgp: &impl PGPProviderSync,
    user_key: &LocalUserKey,
    addr_key: &LocalAddressKey,
    key_pass: &KeySecret,
) -> Result<LocalSignedKeyList, SignupError> {
    let key_id = new_key_id();
    let user_key = user_key.unlock_and_assign_key_id(pgp, key_id.clone(), key_pass)?;
    let addr_key = addr_key.unlock_and_assign_key_id(pgp, key_id.clone(), &user_key)?;
    let addr_skl = LocalSignedKeyList::generate(pgp, &UnlockedAddressKeys(vec![addr_key]))?;

    Ok(addr_skl)
}

fn build_address_key(
    addr: &Address,
    addr_key: &LocalAddressKey,
    addr_skl: &LocalSignedKeyList,
) -> AddressKeyInput {
    let signed_key_list = SignedKeyList {
        data: addr_skl.data.to_string(),
        signature: addr_skl.signature.to_string(),
    };

    AddressKeyInput {
        address_id: addr.id.clone(),
        private_key: addr_key.private_key.to_string(),
        token: addr_key.token.clone().map(|t| t.to_string()),
        signature: addr_key.signature.clone().map(|t| t.to_string()),
        signed_key_list,
        revision: 0,
        primary: 1,
    }
}

/// Generates a dummy key ID.
///
/// This is a bit annoying in the current crypto APIs, you have to pass a dummy `KeyID` to use them.
/// In theory we could introduce another model, but I think it would be an overkill.
/// For the sign-up operations a key with a dummy key id is fine.
fn new_key_id() -> KeyId {
    KeyId(String::default())
}

fn create_payload(
    challenge_info: &ChallengeInfo,
    username_behavior: Option<UsernameBehavior>,
) -> Option<HashMap<String, PayloadFrame>> {
    if username_behavior.is_none() && challenge_info.recovery_behavior.is_none() {
        return None;
    }

    let mut payload = HashMap::with_capacity(2);

    if let Some(behavior) = challenge_info.recovery_behavior.clone() {
        insert_payload_frame(
            &mut payload,
            PayloadFrameType::Recovery,
            challenge_info,
            behavior,
        );
    }

    if let Some(behavior) = username_behavior {
        insert_payload_frame(
            &mut payload,
            PayloadFrameType::Username,
            challenge_info,
            behavior,
        );
    }

    Some(payload)
}

fn insert_payload_frame(
    payload: &mut HashMap<String, PayloadFrame>,
    ty: PayloadFrameType,
    challenge_info: &ChallengeInfo,
    behavior: impl Into<PayloadFrameBehavior>,
) {
    let id = payload.len();
    let name = format!("{}-challenge-{id}", challenge_info.product_version);
    payload.insert(
        name,
        PayloadFrame {
            version: challenge_info.payload_version.into(),
            metadata: PayloadFrameMetadata { ty },
            device_info: challenge_info.device_info.clone(),
            user_behavior: Some(behavior.into()),
        },
    );
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use proton_core_common::device::DeviceInfo;

    use crate::{
        prelude::{
            PayloadFrame, PayloadFrameMetadata, PayloadFrameType, RecoveryBehavior,
            UsernameBehavior,
        },
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
        let username_behavior = UsernameBehavior {
            time_on_field: vec![123],
            click_on_field: 12,
            copy_field: vec!["usr_cf".into()],
            paste_field: vec!["usr_pf".into()],
            key_down_field: vec!["usr_kdf".into()],
        };
        let recovery_behavior = RecoveryBehavior {
            time_on_field: vec![456],
            click_on_field: 34,
            copy_field: vec!["rec_cf".into()],
            paste_field: vec!["rec_pf".into()],
            key_down_field: vec!["rec_kdf".into()],
        };
        let challenge_info = ChallengeInfo {
            payload_version: "1.0",
            product_version: "mail-v1".into(),
            device_info: Some(device_info.clone()),
            recovery_behavior: Some(recovery_behavior.clone()),
        };
        let payload = create_payload(&challenge_info, Some(username_behavior.clone()));
        assert_eq!(
            payload,
            Some(HashMap::from_iter([
                (
                    "mail-v1-challenge-0".to_string(),
                    PayloadFrame {
                        version: "1.0".into(),
                        metadata: PayloadFrameMetadata {
                            ty: PayloadFrameType::Recovery,
                        },
                        device_info: Some(device_info.clone()),
                        user_behavior: Some(recovery_behavior.into()),
                    }
                ),
                (
                    "mail-v1-challenge-1".to_string(),
                    PayloadFrame {
                        version: "1.0".into(),
                        metadata: PayloadFrameMetadata {
                            ty: PayloadFrameType::Username,
                        },
                        device_info: Some(device_info.clone()),
                        user_behavior: Some(username_behavior.into()),
                    }
                )
            ]))
        );
    }
}
