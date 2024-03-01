use crate::auth::ArcAuthStore;
use crate::domain::{EventId, IsEvent, Uid, User, UserSettings};
use crate::http;
use crate::http::{Client, OwnedRequest, RequestDesc, X_PM_UID_HEADER};
use crate::requests::{
    AuthRefreshRequest, CaptchaRequest, GetEventRequest, GetLatestEventRequest,
    GetUserSaltsRequest, LogoutRequest, UserInfoRequest, UserSettingsRequest,
};
use anyhow::anyhow;
use proton_crypto_account::salts::Salts;
use secrecy::ExposeSecret;

/// Authenticated Session from which one can access data/functionality restricted to authenticated
/// users.
#[derive(Clone)]
pub struct Session {
    auth_store: ArcAuthStore,
    client: Client,
}

impl Session {
    pub fn new(client: Client, auth_store: ArcAuthStore) -> Self {
        Self { auth_store, client }
    }

    pub fn auth_store(&self) -> &ArcAuthStore {
        &self.auth_store
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

    pub async fn ping(&self) -> Result<(), http::HttpRequestError> {
        self.client
            .execute_request(crate::requests::Ping {}.to_request())
            .await
    }

    pub async fn captcha_get(
        &self,
        token: &str,
        force_web: bool,
    ) -> Result<String, http::HttpRequestError> {
        self.client
            .execute_request(CaptchaRequest::new(token, force_web).to_request())
            .await
    }

    pub async fn execute_request<'a, 'b: 'a, R: RequestDesc + 'a>(
        &'b self,
        r: R,
    ) -> Result<R::Output, http::HttpRequestError> {
        wrap_session_request(&self.client, self, r).await
    }
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
