#![allow(clippy::wildcard_imports)]
#![allow(clippy::similar_names)]

use crate::AccountApi;
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
use derive_more::Display;
use futures::TryFutureExt;
use muon::Client;
#[allow(deprecated)]
use muon::client::flow::LoginExtraInfo;
use muon::client::flow::LoginFlow;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::SessionId;
use proton_core_api::services::proton::UserId;
use proton_core_api::store::AuthInfo;
use proton_core_api::store::DynStore;
use proton_core_api::store::MbpMode;
use proton_core_api::store::TfaMode;
use proton_core_api::store::UserData;
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::salts::KeySecret;

/// Represents the state where the user can provide recovery information.
#[derive(Debug, Display, Clone)]
#[display("WantCreate")]
pub struct WantCreate {
    client: Client,
    username: Username,
    password: SecureString,
    recovery: Recovery,
    data: StateData,
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
        }
    }

    #[allow(deprecated)]
    pub async fn create(self, store: DynStore) -> StateResult {
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

        let client = match flow {
            LoginFlow::Ok(client, data) => {
                info!("Login successful after signup");

                let info = AuthInfo {
                    user_id: UserId::from(user.id.clone()),
                    session_id: SessionId::from(data.session_id),
                    tfa_mode: TfaMode::none(),
                    mbp_mode: MbpMode::from(data.password_mode),
                    fido_details: None,
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
