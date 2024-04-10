use crate::auth::ArcAuthStore;
use crate::domain::{Event, EventId, User, UserSettings};
use crate::http::{self, APIEnvConfig};
use crate::http::{Client, FromResponse, OwnedRequest, RequestDesc, X_PM_UID_HEADER};
use crate::requests::{
    AuthRefresh, CaptchaRequest, GetEventRequest, GetLatestEventRequest, GetUserSaltsRequest,
    LogoutRequest, UserInfoRequest, UserSettingsRequest,
};
use anyhow::anyhow;
use proton_crypto_account::salts::Salts;

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

    /// Get the session authentication storage.
    #[must_use]
    pub fn auth_store(&self) -> &ArcAuthStore {
        &self.auth_store
    }

    /// Get the API environment info.
    #[must_use]
    pub fn api_env_config(&self) -> &APIEnvConfig {
        &self.client.info().env_config
    }

    /// Get the user details.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn get_user(&self) -> Result<User, http::RequestError> {
        self.execute_request(UserInfoRequest {})
            .await
            .map(|r| r.user)
    }

    /// Get the user salts.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn get_user_salts(&self) -> Result<Salts, http::RequestError> {
        self.execute_request(GetUserSaltsRequest {})
            .await
            .map(|v| v.key_salts)
    }

    /// Logout the user and invalidate the current session.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn logout(&self) -> Result<(), http::RequestError> {
        self.execute_request(LogoutRequest {}).await?;
        self.auth_store.write().await.clear_auth().map_err(|e| {
            http::RequestError::Other(anyhow!("Failed to remove auth from store: {e}"))
        })?;
        Ok(())
    }

    /// Get the latest event id.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn get_latest_event(&self) -> Result<EventId, http::RequestError> {
        self.execute_request(GetLatestEventRequest {})
            .await
            .map(|r| r.event_id)
    }

    /// Get the event with the given id.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn get_event<T: Event>(&self, id: &EventId) -> Result<T, http::RequestError> {
        self.execute_request(GetEventRequest::new(id)).await
    }

    /// Get the event with the given id and add conversation and messages counts.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn get_event_with_conv_and_msg_counts<T: Event>(
        &self,
        id: &EventId,
    ) -> Result<T, http::RequestError> {
        self.execute_request(GetEventRequest::with_counts(id)).await
    }

    /// Get the user's settings.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn get_user_settings(&self) -> Result<UserSettings, http::RequestError> {
        self.execute_request(UserSettingsRequest {})
            .await
            .map(|v| v.user_settings)
    }

    /// Get the event with the given id.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn ping(&self) -> Result<(), http::RequestError> {
        self.client
            .execute_request(crate::requests::Ping {}.to_request())
            .await
    }

    /// Get the captcha web page to display in the web view.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn captcha_get(
        &self,
        token: &str,
        force_web: bool,
    ) -> Result<String, http::RequestError> {
        self.client
            .execute_request(CaptchaRequest::new(token, force_web).to_request())
            .await
    }

    /// Execute the given request with this session.
    ///
    /// # Errors
    /// Returns error if the request failed.
    pub async fn execute_request<'a, 'b: 'a, R: RequestDesc + 'a>(
        &'b self,
        r: R,
    ) -> Result<<R::Response as FromResponse>::Output, http::RequestError> {
        wrap_session_request(&self.client, self, r).await
    }
}

async fn wrap_session_request<'a, R: RequestDesc + 'a>(
    client: &Client,
    session: &'a Session,
    r: R,
) -> Result<<R::Response as FromResponse>::Output, http::RequestError> {
    let r = r.build();
    // Get the current auth version before making this call.
    let (data, auth_version) = {
        let borrow = session.auth_store.read().await;
        let auth_version = borrow.auth_refresh_version();
        (
            if let Some(auth) = borrow.get_auth() {
                r.header(X_PM_UID_HEADER, auth.uid.as_ref())
                    .bearer_token(auth.access_token.expose_secret())
            } else {
                r
            },
            auth_version,
        )
    };

    // While we clone headers and url, the body clone is handled efficiently.
    match client
        .execute_request(OwnedRequest::<R::Response>::new(data.clone()))
        .await
    {
        Ok(v) => Ok(v),
        Err(e) => {
            if let http::RequestError::API(api_err) = &e {
                if api_err.http_code == 401 {
                    tracing::debug!("Account session expired, attempting refresh");
                    let data = {
                        let mut refresh_guard = session.auth_store.write().await;
                        // If the version still matches the auth store version, it means we are the first to attempt refresh.
                        if auth_version == refresh_guard.auth_refresh_version() {
                            tracing::debug!("Version still matches, refreshing");
                            let auth_refresh_request = {
                                let Some(auth) = refresh_guard.get_auth() else {
                                    let e =
                                        anyhow!("Refresh was request but there is no auth token");
                                    tracing::error!("{e}");
                                    return Err(http::RequestError::Other(e));
                                };
                                let request =
                                    AuthRefresh::new(&auth.uid, auth.refresh_token.expose_secret())
                                        .to_request();
                                request
                            };

                            // Refresh the token.
                            let auth_refresh_response = client
                                .execute_request(auth_refresh_request)
                                .await
                                .map_err(|e| {
                                    tracing::error!("Failed to refresh token: {e}");
                                    refresh_guard.refresh_auth_failed(&e);
                                    e
                                })?;

                            // Store the new token.
                            refresh_guard
                                .refresh_auth(
                                    auth_refresh_response.uid,
                                    auth_refresh_response.access_token,
                                    auth_refresh_response.refresh_token,
                                    auth_refresh_response.scope,
                                )
                                .map_err(|e| {
                                    http::RequestError::Other(anyhow!("Failed to store auth: {e}"))
                                })?;
                        }
                        tracing::debug!("Session has already been refreshed");
                        let Some(auth) = refresh_guard.get_auth() else {
                            let e = anyhow!("Refresh was request but there is no auth token");
                            tracing::error!("{e}");
                            return Err(http::RequestError::Other(e));
                        };
                        data.header(X_PM_UID_HEADER, auth.uid.as_ref())
                            .bearer_token(auth.access_token.expose_secret())
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
