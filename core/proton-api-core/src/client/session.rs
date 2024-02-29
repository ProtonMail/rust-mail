use crate::auth::ArcAuthStore;
use crate::client::TotpSession;
use crate::domain::{
    EventId, HumanVerification, HumanVerificationLoginData, IsEvent, TFAStatus, TwoFactorAuth, Uid,
    User, UserSettings,
};
use crate::http;
use crate::http::{Client, OwnedRequest, RequestDesc, X_PM_UID_HEADER};
use crate::requests::{
    AuthInfoRequest, AuthRefreshRequest, AuthRequest, AuthResponse, GetEventRequest,
    GetLatestEventRequest, GetUserSaltsRequest, LogoutRequest, TOTPRequest, UserInfoRequest,
    UserSettingsRequest,
};
use anyhow::anyhow;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::Salts;
use secrecy::ExposeSecret;

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

pub enum SessionType {
    Authenticated(Session),
    AwaitingTotp(TotpSession),
}

/// Authenticated Session from which one can access data/functionality restricted to authenticated
/// users.
#[derive(Clone)]
pub struct Session {
    auth_store: ArcAuthStore,
    client: Client,
}

impl Session {
    fn new(client: Client, auth_store: ArcAuthStore) -> Self {
        Self { auth_store, client }
    }

    pub async fn login<'a>(
        c: Client,
        auth_store: ArcAuthStore,
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

        validate_server_proof(c, auth_store, &proof, auth_req_res)
            .map_err(map_human_verification_err)
    }

    pub async fn submit_totp(&self, code: &str) -> Result<(), http::HttpRequestError> {
        self.execute_request(TOTPRequest::new(code)).await
    }

    pub async fn refresh(
        c: Client,
        auth_store: ArcAuthStore,
        user_uid: &Uid,
        token: &str,
    ) -> Result<Self, http::HttpRequestError> {
        let client = c.clone();
        c.execute_request(AuthRefreshRequest::new(user_uid, token).to_request())
            .await
            .map(move |r| {
                auth_store
                    .write()
                    .set_auth(r.uid, r.refresh_token.0, r.access_token.0, r.scope);
                Session::new(client, auth_store)
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
        self.execute_request(LogoutRequest {}).await?;
        self.auth_store.write().clear_auth();
        Ok(())
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
        self.execute_request(UserSettingsRequest {})
            .await
            .map(|v| v.user_settings)
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
    auth_store: ArcAuthStore,
    proof: &proton_crypto_account::proton_crypto::srp::ClientProof,
    auth_response: AuthResponse,
) -> Result<SessionType, LoginError> {
    if proof.expected_server_proof != auth_response.server_proof {
        return Err(LoginError::ServerProof(
            "Server Proof does not match".to_string(),
        ));
    }

    let tfa_enabled = auth_response.tfa.enabled;
    {
        auth_store.write().set_auth(
            auth_response.uid,
            auth_response.refresh_token.0,
            auth_response.access_token.0,
            auth_response.scope,
        );
    }

    let session = Session::new(client, auth_store);

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
    let r = r.build();
    let data = {
        let borrow = session.auth_store.read();
        if let Some(auth) = borrow.get_auth() {
            r.header(X_PM_UID_HEADER, auth.uid.as_ref())
                .bearer_token(auth.access_token.expose_secret())
        } else {
            r
        }
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
                        let reader = session.auth_store.read();
                        let Some(auth) = reader.get_auth() else {
                            return Err(http::HttpRequestError::Other(anyhow!(
                                "Authentication info missing for refresh"
                            )));
                        };
                        let request =
                            AuthRefreshRequest::new(&auth.uid, auth.refresh_token.expose_secret())
                                .to_request();
                        request
                    };

                    let auth_refresh_response =
                        client.execute_request(auth_refresh_request).await?;
                    let data = {
                        let mut writer = session.auth_store.write();
                        let data = data
                            .header(X_PM_UID_HEADER, auth_refresh_response.uid.as_ref())
                            .bearer_token(auth_refresh_response.access_token.0.expose_secret());
                        writer.set_auth(
                            auth_refresh_response.uid,
                            auth_refresh_response.refresh_token.0,
                            auth_refresh_response.access_token.0,
                            auth_refresh_response.scope,
                        );
                        data
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
