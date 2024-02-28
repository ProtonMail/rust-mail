use crate::clientv2::TotpSession;
use crate::domain::{
    EventId, HumanVerification, HumanVerificationLoginData, IsEvent, TFAStatus, TwoFactorAuth,
    User, UserSettings, UserUid,
};
use crate::http;
use crate::http::{Client, OwnedRequest, RequestDesc, X_PM_UID_HEADER};
use crate::requests::{
    AuthInfoRequest, AuthRefreshRequest, AuthRequest, AuthResponse, GetEventRequest,
    GetLatestEventRequest, GetUserSaltsRequest, LogoutRequest, TOTPRequest, UserAuth,
    UserInfoRequest, UserSettingsRequest,
};
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::Salts;
use secrecy::{ExposeSecret, Secret};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
#[cfg_attr(feature = "uniffi", uniffi(flat_error))]
pub enum LoginError {
    #[error("{0}")]
    Request(
        #[from]
        #[source]
        http::HttpRequestError,
    ),
    #[error("Server SRP proof verification failed: {0}")]
    ServerProof(String),
    #[error("Account 2FA method ({0})is not supported")]
    Unsupported2FA(TwoFactorAuth),
    #[error("Human Verification Required'")]
    HumanVerificationRequired(HumanVerification),
    #[error("Failed to calculate SRP Proof: {0}")]
    SRPProof(String),
}

/// Data which can be used to save a session and restore it later.
pub struct SessionRefreshData {
    pub user_uid: Secret<UserUid>,
    pub token: Secret<String>,
}

impl PartialEq for SessionRefreshData {
    fn eq(&self, other: &Self) -> bool {
        self.user_uid.expose_secret() == other.user_uid.expose_secret()
            && self.token.expose_secret() == other.token.expose_secret()
    }
}

impl Eq for SessionRefreshData {}

#[derive(Debug)]
pub enum SessionType {
    Authenticated(Session),
    AwaitingTotp(TotpSession),
}

/// Authenticated Session from which one can access data/functionality restricted to authenticated
/// users.
#[derive(Debug, Clone)]
pub struct Session {
    pub(super) user_auth: Arc<parking_lot::RwLock<UserAuth>>,
    client: Client,
}

impl Session {
    fn new(client: Client, user: UserAuth) -> Self {
        Self {
            user_auth: Arc::new(parking_lot::RwLock::new(user)),
            client,
        }
    }

    pub async fn login<'a>(
        c: Client,
        username: &'a str,
        password: &'a str,
        human_verification: Option<HumanVerificationLoginData>,
    ) -> Result<SessionType, LoginError> {
        let auth_resp = c
            .execute_request(AuthInfoRequest { username }.to_request())
            .await?;

        let srp_provider = proton_crypto_account::proton_crypto::new_srp_provider();
        let proof = srp_provider
            .generate_client_proof(
                username,
                password,
                auth_resp.version,
                &auth_resp.salt,
                &auth_resp.modulus,
                &auth_resp.server_ephemeral,
            )
            .map_err(|e| LoginError::SRPProof(e.to_string()))?;

        let auth_req_res = c
            .execute_request(
                AuthRequest {
                    username,
                    client_ephemeral: &proof.ephemeral,
                    client_proof: &proof.proof,
                    srp_session: &auth_resp.srp_session,
                    human_verification: &human_verification,
                }
                .to_request(),
            )
            .await?;

        validate_server_proof(c, &proof, auth_req_res).map_err(map_human_verification_err)
    }

    pub async fn submit_totp(&self, code: &str) -> Result<(), http::HttpRequestError> {
        self.execute_request(TOTPRequest::new(code)).await
    }

    pub async fn refresh(
        c: Client,
        user_uid: &UserUid,
        token: &str,
    ) -> Result<Self, http::HttpRequestError> {
        let client = c.clone();
        c.execute_request(AuthRefreshRequest::new(user_uid, token).to_request())
            .await
            .map(move |r| {
                let user = UserAuth::from_auth_refresh_response(r);
                Session::new(client, user)
            })
    }

    pub async fn get_user(&self) -> Result<User, http::HttpRequestError> {
        self.execute_request(UserInfoRequest {})
            .await
            .map(|r| r.user)
    }

    pub async fn get_user_salts(&self) -> Result<Salts, http::HttpRequestError> {
        self.execute_request(GetUserSaltsRequest {})
            .await
            .map(|v| v.key_salts)
    }

    pub async fn logout(&self) -> Result<(), http::HttpRequestError> {
        self.execute_request(LogoutRequest {}).await
    }

    pub async fn get_latest_event(&self) -> Result<EventId, http::HttpRequestError> {
        self.execute_request(GetLatestEventRequest {})
            .await
            .map(|r| r.event_id)
    }

    pub async fn get_event<T: IsEvent>(&self, id: &EventId) -> Result<T, http::HttpRequestError> {
        self.execute_request(GetEventRequest::new(id)).await
    }

    pub async fn get_user_settings(&self) -> Result<UserSettings, http::HttpRequestError> {
        self.execute_request(UserSettingsRequest {}).await
    }

    pub fn get_refresh_data(&self) -> SessionRefreshData {
        let reader = self.user_auth.read();
        SessionRefreshData {
            user_uid: reader.uid.clone(),
            token: reader.refresh_token.clone(),
        }
    }

    pub async fn execute_request<'a, 'b: 'a, R: RequestDesc + 'a>(
        &'b self,
        r: R,
    ) -> Result<R::Output, http::HttpRequestError> {
        wrap_session_request(&self.client, self, r).await
    }
}

fn validate_server_proof(
    client: Client,
    proof: &proton_crypto_account::proton_crypto::srp::ClientProof,
    auth_response: AuthResponse,
) -> Result<SessionType, LoginError> {
    if proof.expected_server_proof != auth_response.server_proof {
        return Err(LoginError::ServerProof(
            "Server Proof does not match".to_string(),
        ));
    }

    let tfa_enabled = auth_response.tfa.enabled;
    let user = UserAuth::from_auth_response(auth_response);

    let session = Session::new(client, user);

    match tfa_enabled {
        TFAStatus::None => Ok(SessionType::Authenticated(session)),
        TFAStatus::Totp => Ok(SessionType::AwaitingTotp(TotpSession(session))),
        TFAStatus::FIDO2 => Err(LoginError::Unsupported2FA(TwoFactorAuth::FIDO2)),
        TFAStatus::TotpOrFIDO2 => Ok(SessionType::AwaitingTotp(TotpSession(session))),
    }
}

fn map_human_verification_err(e: LoginError) -> LoginError {
    if let LoginError::Request(http::HttpRequestError::API(e)) = &e {
        if let Ok(hv) = e.try_get_human_verification_details() {
            return LoginError::HumanVerificationRequired(hv);
        }
    }

    e
}

async fn wrap_session_request<'a, R: RequestDesc + 'a>(
    client: &Client,
    session: &'a Session,
    r: R,
) -> Result<R::Output, http::HttpRequestError> {
    let data = {
        let borrow = session.user_auth.read();
        r.build()
            .header(X_PM_UID_HEADER, borrow.uid.expose_secret().as_ref())
            .bearer_token(borrow.access_token.expose_secret())
    };

    // While we clone headers and url, the body clone is handled efficiently.
    match client
        .execute_request(OwnedRequest::<R::Response>::new(data.clone()))
        .await
    {
        Ok(v) => Ok(v),
        Err(e) => {
            if let http::HttpRequestError::API(api_err) = &e {
                if api_err.http_code == 401 {
                    tracing::debug!("Account session expired, attempting refresh");

                    let auth_refresh_request = {
                        let borrow = session.user_auth.read();
                        AuthRefreshRequest::new(
                            borrow.uid.expose_secret(),
                            borrow.refresh_token.expose_secret(),
                        )
                        .to_request()
                    };

                    let auth_refresh_response =
                        client.execute_request(auth_refresh_request).await?;
                    let data = {
                        let mut writer = session.user_auth.write();
                        *writer = UserAuth::from_auth_refresh_response(auth_refresh_response);
                        data.header(X_PM_UID_HEADER, writer.uid.expose_secret().as_ref())
                            .bearer_token(writer.access_token.expose_secret())
                    };

                    return client
                        .execute_request(OwnedRequest::<R::Response>::new(data))
                        .await;
                }
            }

            Err(e)
        }
    }
}
