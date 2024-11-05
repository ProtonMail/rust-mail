#![allow(clippy::module_name_repetitions)]

//! The Proton API service.
//!
//! This module provides a service that can be used to make requests to the
//! Proton API. Each method provided should match 1:1 with an API endpoint, and
//! follow the naming convention of the endpoint. For example, the endpoint
//! `GET /contacts` should have a method provided called `get_contacts()`.
//!
//! The purpose of the API service is to provide not only the means to make
//! requests, but also a formalisation of the data that is sent and received. To
//! this end, the data structures provided by this service should mirror the API
//! endpoint definitions, and NOT have any business logic or other
//! functionality.
//!
//! To be clear, they should only contain data, and not methods; should not be
//! saved in the database; and should not be used for anything except providing
//! an interface for data exchange.
//!
//! The goal is not to provide a semantic representation of actions, but a
//! strict and closely-coupled interface to the API.
//!
//! Everything in this service should be self-contained as much as possible, and
//! should be considered encapsulated and separate from the main application,
//! including the application's data. Types should be converted back and forth
//! as necessary, but generally not used in both places.
//!
//! # Example illustration
//!
//! Let's consider the case of a user. The application may have a `User` struct
//! that is used to represent a user in the application. From time to time it
//! will be necessary to interact with the API to sync data relevant to that
//! `User`. To do this, the necessary information should be used to prepare data
//! to send to the API, such as a `PostUserRequest` struct containing a child
//! data type of `User`. This latter `User` is not the same struct as that used
//! in the application, but rather, a data-only mirroring of the data the API
//! needs to receive.
//!
//! Now let's consider retrieving a user record. The API response might define
//! a `User` structure that is the same as the one accepted via `POST` — if so,
//! this could go into [`common`]. Otherwise, we will need two `User` structs,
//! one in [`request_data`] and one in [`response_data`]. Neither one of these
//! is the same as the one used inside the main application.
//!
//! Once the data is retrieved, the data required by the application can be
//! extracted from the response and converted into the application's `User`
//! struct. This struct would then be the one containing various methods and
//! other functionality, and would get saved to the database.
//!

pub mod common;
pub mod request_data;
pub mod requests;
pub mod response_data;
pub mod responses;

use crate::auth::{CachedStore, Store, StoreError};
use crate::service::{
    ApiResponse, ApiService, ApiServiceError, Body, Json, Request, ServiceError, NO_PARAMS,
};
use crate::services::proton::common::{Fido2Auth, RemoteId};
use crate::services::proton::request_data::HumanVerificationData;
use crate::services::proton::requests::{
    GetCaptchaOptions, GetContactsEmailsOptions, GetContactsOptions, GetEventOptions,
    GetImagesLogoOptions, GetKeysAllOptions, PostAuthInfoRequest, PostAuthRefreshRequest,
    PostAuthRequest, PostAuthSessionsForksRequest, PostAuthTfaRequest,
};
use crate::services::proton::response_data::{ApiErrorInfo, HumanVerificationChallenge};
use crate::services::proton::responses::{
    GetAddressesResponse, GetContactResponse, GetContactsEmailsResponse, GetContactsResponse,
    GetEventResponse, GetEventsLatestResponse, GetKeysSaltsResponse, GetSettingsResponse,
    GetUsersResponse, PostAuthInfoResponse, PostAuthRefreshResponse, PostAuthResponse,
    PostAuthSessionsForksResponse, PostAuthTfaResponse,
};
use crate::{
    DEFAULT_APP_VERSION, DEFAULT_CLIENT, DEFAULT_HOST_URL, DEFAULT_REDIRECT_URL,
    X_PM_APP_VERSION_HEADER, X_PM_HUMAN_VERIFICATION_TOKEN, X_PM_HUMAN_VERIFICATION_TOKEN_TYPE,
};
use bytes::Bytes;
use parking_lot::RwLock;
use proton_crypto_account::keys::APIPublicAddressKeys;
use requests::PutDeleteContacts;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, Url};
use responses::{GetAddressResponse, PutDeleteContactsResponse};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::{Error as JsonError, Value as JsonValue};
use smart_default::SmartDefault;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock as AsyncRwLock;
use tracing::error;
use velcro::hash_map;

const HUMAN_VERIFICATION_REQUESTED: u32 = 9001;

#[derive(Debug, Error)]
pub enum ProtonApiServiceError {
    //  HUMAN VERIFICATION DATA ERRORS
    //==========================================================================
    /// Human verification data was returned, but could not be deserialised.
    #[error("Failed to deserialize human verification data: {0}")]
    FailedToDeserializeHumanVerificationData(JsonError),

    /// Human verification has been requested — this should lead to this
    /// particular error being detected and handled.
    #[error("Human verification requested")]
    HumanVerificationRequested(HumanVerificationChallenge),

    /// Human verification was indicated, but the data is missing.
    #[error("Missing human verification data")]
    MissingHumanVerificationData,

    /// Human verification was indicated, but the specified type is unknown.
    #[error(r#"Unknown human verification type "{0}""#)]
    UnknownHumanVerificationType(String),
}

impl ServiceError for ProtonApiServiceError {}

/// The configuration for the Proton API service.
#[derive(Clone, Debug, Eq, PartialEq, SmartDefault)]
pub struct Config {
    /// TODO: Document this field.
    pub allow_http: bool,

    /// TODO: Document this field.
    #[default(DEFAULT_APP_VERSION.to_owned())]
    pub app_version: String,

    /// The base URL for the external service.
    #[default(DEFAULT_HOST_URL.to_owned())]
    pub base_url: String,

    /// TODO: Document this field.
    pub skip_srp_proof_validation: bool,

    /// TODO: Document this field.
    #[default(DEFAULT_CLIENT.to_owned())]
    pub user_agent: String,
}

/// A service for communicating with the Proton API.
///
/// This struct is thread-safe, and can be cloned without issue. Cloning will
/// create a new Reqwest [`Client`] instance from the pool. It will also provide
/// a new shared reference to the persistent headers, which are shared between
/// threads, but the base URL is not expected to change after instantiation and
/// is not shared.
///
#[derive(Clone)]
pub struct Proton {
    /// The current authentication context.
    auth: Arc<AsyncRwLock<CachedStore>>,

    /// The base URL for the external service.
    base_url: Url,

    /// The Reqwest HTTP client which is used internally.
    client: Client,

    /// The configuration for the service.
    config: Config,

    /// A collection of headers to send with every request.
    headers: Arc<RwLock<HeaderMap>>,
}

impl ApiService for Proton {
    fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn client(&self) -> &Client {
        &self.client
    }

    fn headers(&self) -> HeaderMap {
        self.headers.read().clone()
    }

    async fn on_error<J, T>(
        &self,
        error: ApiServiceError,
        request: Request<J>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: ApiResponse,
    {
        // At present, the only extended error details that we are interested in
        // are those for Unauthorized statuses, because if we have an active,
        // authenticated session, it means it has expired, and we need to
        // refresh the token.
        match &error {
            ApiServiceError::Unauthorized(_, _) => {
                // In a situation where we receive a 401 Unauthorized, we need to check if
                // the session has expired. If it has, we need to refresh the token and
                // retry the request. Otherwise, it means we are not authenticated, and we
                // need to log in, so we return the error as is.
                //
                // For reference, the API documentation for this endpoint is here:
                // https://confluence.protontech.ch/display/API/Authentication%2C+sessions%2C+and+tokens
                //
                // First we obtain a write lock to ensure no other threads are able to try
                // to refresh the token at the same time. Meanwhile, during the period from
                // the first failure (when the token expired) to the time when the token is
                // refreshed, all requests will fail with the same error. This means that
                // other threads will get to this piece of code and will also try to refresh
                // the token. This can lead to a situation where the token is refreshed
                // multiple times.
                //
                // To avoid this being a problem, we check the status of the lock. The only
                // situation in which the lock is busy is when the token is being refreshed.
                // This means that another thread is already refreshing the token, and we
                // don't need to do anything. In this case, we just wait for the lock to be
                // released, and then we can retry the request.
                let mut refresh_already_in_progress = false;
                #[allow(clippy::single_match_else)]
                let mut auth_store = match self.auth.try_write() {
                    Ok(lock) => lock,
                    Err(_) => {
                        refresh_already_in_progress = true;
                        // Wait for the lock to be released
                        self.auth.write().await
                    }
                };
                // We take the auth details, and put them back later. Meanwhile, if the
                // refresh attempt fails, we're not logged in anyway, and so clearing the
                // auth value at this stage is the right thing to do.
                let (Some(mut auth), Some(secrets)) = auth_store.clear().await? else {
                    error!("Token refresh was requested, but there is no auth token");
                    // Return the original error, i.e. a 401 Unauthorized, if there is no token.
                    return Err(error);
                };
                // We only carry out the refresh if it is not already in progress elsewhere.
                if refresh_already_in_progress {
                    tracing::debug!("Account session expired, refresh is already in progress");
                } else {
                    tracing::debug!("Account session expired, attempting refresh");
                    // Refresh the token. In order to avoid a nested calling situation, we do
                    // this manually, and not by calling post_auth_refresh(). This is not ideal,
                    // as it creates a slight duplication of logic, but is acceptable for now in
                    // order to avoid recursion.
                    let request = Request {
                        body: Body::Json(PostAuthRefreshRequest {
                            uid: auth.uid.clone(),
                            refresh_token: auth.refresh_token.expose_secret().to_owned(),
                            grant_type: "refresh_token".to_owned(),
                            response_type: "token".to_owned(),
                            redirect_uri: DEFAULT_REDIRECT_URL.to_owned(),
                        }),
                        method: Method::POST,
                        url: self.get_url("auth/v4/refresh", None),
                        ..Default::default()
                    };
                    let response = Self::handle_response::<Json<PostAuthRefreshResponse>>(
                        Self::send_request(self.prepare_request(&request)?)
                            .await
                            .map_err(|err| {
                                error!("Failed to refresh token: {err}");
                                // Return the original error, i.e. a 401 Unauthorized, if the token refresh
                                // failed.
                                error
                            })?,
                    )
                    .await?;
                    // Store the new token.
                    auth.uid = response.uid;
                    auth.access_token = response.access_token;
                    auth.refresh_token = response.refresh_token;
                    auth.auth_scope = response.scopes;
                    tracing::debug!("Session has been refreshed");
                }
                if let Err(e) = auth_store.set_auth_session(auth).await {
                    error!("Failed to update authentication in store: {e}");
                }
                if let Err(e) = auth_store.set_user_secrets(secrets).await {
                    error!("Failed to update secrets in store: {e}");
                }
                drop(auth_store);
                // Retry the request. Note that there is a potential for an infinite loop
                // here if the token refresh fails, and although there should not be a
                // situation where the refresh succeeds and then the retry gets another 401
                // Unauthorized, we track the number of retries in order to prevent this.
                self.retry_request::<J, T>(request).await
            }
            // At present, the only extended error details that we are interested in
            // are those for UnprocessableEntity statuses, as these can contain details
            // for human verification, which we then need to carry out. This response
            // can happen at any time.
            ApiServiceError::UnprocessableEntity(_, body) => {
                match serde_json::from_str::<ApiErrorInfo>(body) {
                    Ok(info) => {
                        // When we encounter a human verification challenge, we need to perform the
                        // relevant actions for the specified type of human verification.
                        if info.code == HUMAN_VERIFICATION_REQUESTED {
                            let Some(details) = info.details else {
                                return Err(ApiServiceError::ServiceError(Box::new(
                                    ProtonApiServiceError::MissingHumanVerificationData,
                                )));
                            };
                            Err(ApiServiceError::ServiceError(Box::new(
                                ProtonApiServiceError::HumanVerificationRequested(
                                    match serde_json::from_value::<HumanVerificationChallenge>(
                                        details.clone(),
                                    ) {
                                        Ok(data) => data,
                                        Err(err) => return Err(ApiServiceError::ServiceError(Box::new(
                                            ProtonApiServiceError::FailedToDeserializeHumanVerificationData(err),
                                        )))
                                    }
                                ),
                            )))
                        } else {
                            Err(error)
                        }
                    }
                    Err(err) => {
                        // If we couldn't parse the response, we return it unaltered, as we don't
                        // know if it contains a human verification challenge unless we can
                        // deserialise it.
                        error!("Failed to parse error response: {err}");
                        Err(error)
                    }
                }
            }
            _ => Err(error),
        }
    }

    fn set_header(&self, name: &str, value: &str) {
        self.headers.write().insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_bytes(value.as_bytes()).unwrap(),
        );
    }
}

impl Proton {
    const BASE_PATH: &'static str = "core/v4";

    /// Generates a new external API service handler.
    ///
    /// # Parameters
    ///
    /// * `config`  - The API configuration options.
    /// * `headers` - The headers to send with every request.
    /// * `store`   - The authentication storage implementation.
    ///
    /// # Errors
    ///
    /// Returns error if we fail to read existing authentication from the store.
    #[allow(clippy::missing_panics_doc)]
    pub async fn new(
        config: Config,
        headers: Option<HeaderMap>,
        store: Option<Box<dyn Store>>,
    ) -> Result<Self, StoreError> {
        let base_url = Url::parse(&config.base_url).unwrap();
        let headers = Arc::new(RwLock::new(headers.unwrap_or_default()));
        // Insert app version header.
        {
            headers.write().insert(
                X_PM_APP_VERSION_HEADER,
                HeaderValue::from_str(&config.app_version).map_err(Box::new)?,
            );
        }
        let auth = Arc::new(AsyncRwLock::new(
            CachedStore::new(store, Arc::clone(&headers)).await?,
        ));
        Ok(Self {
            auth,
            base_url,
            client: Client::new(),
            config,
            headers,
        })
    }

    /// Gets the API configuration options.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// DELETEs the current authentication session.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn delete_auth(&self) -> Result<(), ApiServiceError> {
        self.delete::<_, ()>("auth/v4", NO_PARAMS, None).await
    }

    /// GETs a list of addresses.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_addresses(&self) -> Result<GetAddressesResponse, ApiServiceError> {
        self.get::<_, Json<_>>(&format!("{}/addresses", Self::BASE_PATH), NO_PARAMS, None)
            .await
    }

    /// GET a single address
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_address_by_id(
        &self,
        id: RemoteId,
    ) -> Result<GetAddressResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/addresses/{id}", Self::BASE_PATH),
            NO_PARAMS,
            None,
        )
        .await
    }

    /// GETs Captcha details.
    ///
    /// # Parameters
    ///
    /// * `token`     - The Captcha token to use.
    /// * `force_web` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_captcha(
        &self,
        token: String,
        force_web: bool,
    ) -> Result<String, ApiServiceError> {
        self.get::<_, String>(
            &format!("{}/captcha", Self::BASE_PATH),
            Some(GetCaptchaOptions {
                force_web_messaging: force_web,
                token,
            }),
            None,
        )
        .await
    }

    /// GETs a single contact.
    ///
    /// This returns the full contact record.
    ///
    /// # Parameters
    ///
    /// * `contact_id` - The ID of the contact to get.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_contact(
        &self,
        contact_id: RemoteId,
    ) -> Result<GetContactResponse, ApiServiceError> {
        self.get::<_, Json<_>>(&format!("contacts/{contact_id}"), NO_PARAMS, None)
            .await
    }

    /// GETs a list of contacts.
    ///
    /// This returns basic information — not the full contact record.
    ///
    /// # Parameters
    ///
    /// * `options` - The options to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_contacts(
        &self,
        options: GetContactsOptions,
    ) -> Result<GetContactsResponse, ApiServiceError> {
        self.get::<_, Json<_>>("contacts", Some(options), None)
            .await
    }

    /// GETs a list of emails for contacts.
    ///
    /// This returns basic information — not the full contact record.
    ///
    /// # Parameters
    ///
    /// * `options` - The options to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_contacts_emails(
        &self,
        options: GetContactsEmailsOptions,
    ) -> Result<GetContactsEmailsResponse, ApiServiceError> {
        self.get::<_, Json<_>>("contacts/emails", Some(options), None)
            .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `event_id`            - The ID of the event to get.
    /// * `conversation_counts` - TODO: Document this parameter.
    /// * `message_counts`      - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_event<T>(
        &self,
        event_id: RemoteId,
        conversation_counts: bool,
        message_counts: bool,
    ) -> Result<T, ApiServiceError>
    where
        T: GetEventResponse + for<'de> Deserialize<'de>,
    {
        self.get::<_, Json<_>>(
            &format!("core/v5/events/{event_id}"),
            Some(GetEventOptions {
                conversation_counts,
                message_counts,
            }),
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_events_latest(&self) -> Result<GetEventsLatestResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/events/latest", Self::BASE_PATH),
            NO_PARAMS,
            None,
        )
        .await
    }

    /// Get logo corresponding to an address or a domain.
    ///
    /// # Errors
    ///   * if the request failed.
    pub async fn get_images_logo(
        &self,
        options: GetImagesLogoOptions,
    ) -> Result<Bytes, ApiServiceError> {
        self.get::<_, Bytes>(
            &format!("{}/images/logo", Self::BASE_PATH),
            Some(options),
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `email`         - The email address to get keys for.
    /// * `internal_only` - Whether to only get internal keys.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_keys_all(
        &self,
        email: String,
        internal_only: Option<bool>,
    ) -> Result<APIPublicAddressKeys, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/keys/all", Self::BASE_PATH),
            Some(GetKeysAllOptions {
                email,
                internal_only,
            }),
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_keys_salts(&self) -> Result<GetKeysSaltsResponse, ApiServiceError> {
        self.get::<_, Json<_>>(&format!("{}/keys/salts", Self::BASE_PATH), NO_PARAMS, None)
            .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_settings(&self) -> Result<GetSettingsResponse, ApiServiceError> {
        self.get::<_, Json<_>>(&format!("{}/settings", Self::BASE_PATH), NO_PARAMS, None)
            .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_tests_ping(&self) -> Result<(), ApiServiceError> {
        self.get::<_, ()>("tests/ping", NO_PARAMS, None).await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn get_users(&self) -> Result<GetUsersResponse, ApiServiceError> {
        self.get::<_, Json<_>>(&format!("{}/users", Self::BASE_PATH), NO_PARAMS, None)
            .await
    }

    /// Method requests to delete contacts which remotes ids were provided.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn put_delete_contacts(
        &self,
        ids: Vec<RemoteId>,
    ) -> Result<PutDeleteContactsResponse, ApiServiceError> {
        let body = PutDeleteContacts { ids };

        self.put::<_, Json<_>>("contacts/delete", body, None).await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `body`               - The body to use for the request.
    /// * `human_verification` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn post_auth(
        &self,
        body: PostAuthRequest,
        human_verification: Option<HumanVerificationData>,
    ) -> Result<PostAuthResponse, ApiServiceError> {
        // Repeat submission with x-pm-human-verification-token and
        // x-pm-human-verification-token-type
        let headers = human_verification.as_ref().map(|hv| {
            hash_map! {
                X_PM_HUMAN_VERIFICATION_TOKEN.to_owned(): hv.token.clone(),
                X_PM_HUMAN_VERIFICATION_TOKEN_TYPE.to_owned(): hv.hv_type.as_str().to_owned(),
            }
        });
        self.post::<_, Json<_>>("auth/v4", body, headers).await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `username` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn post_auth_info(
        &self,
        username: String,
    ) -> Result<PostAuthInfoResponse, ApiServiceError> {
        self.post::<_, Json<_>>("auth/v4/info", PostAuthInfoRequest { username }, None)
            .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `uid`           - TODO: Document this parameter.
    /// * `refresh_token` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn post_auth_refresh(
        &self,
        uid: RemoteId,
        refresh_token: String,
    ) -> Result<PostAuthRefreshResponse, ApiServiceError> {
        self.post::<_, Json<_>>(
            "auth/v4/refresh",
            PostAuthRefreshRequest {
                uid,
                refresh_token,
                grant_type: "refresh_token".to_owned(),
                response_type: "token".to_owned(),
                redirect_uri: DEFAULT_REDIRECT_URL.to_owned(),
            },
            None,
        )
        .await
    }

    /// Fork session request.
    ///
    /// This request is used to fork a user's session, providing a new session
    /// for the same user.
    ///
    /// The general documentation for this can currently be found here:
    ///
    ///   - [Feature documentation](https://confluence.protontech.ch/display/CP/How+to+generate+a+session+fork+selector+for+testing+the+lite+account+application)
    ///
    /// The required POST request is described as being:
    ///
    ///   - `POST /api/auth/sessions/forks`
    ///   - `{ ChildClientID: "web-account-lite", Independent: 0 }`
    ///
    /// The headers should be taken care of by the general request-response
    /// process. Therefore all this action needs to do is call the endpoint with
    /// the required JSON body.
    ///
    /// The relevant API documentation is here:
    ///
    ///   - [API docs](https://protonmail.gitlab-pages.protontech.ch/Slim-API/auth/#tag/Authentication-Sessions/operation/post_auth-%7B_version%7D-sessions-forks)
    ///
    /// The fields in the JSON body are not currently documented.
    ///
    /// # Parameters
    ///
    /// * `child_client_id` - The child client ID to use for the request, which
    ///                       is always `"web-account-lite"` at present. It
    ///                       seems like this is an identifier for the caller,
    ///                       but this is not clear.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn post_auth_sessions_forks(
        &self,
        child_client_id: Option<String>,
    ) -> Result<PostAuthSessionsForksResponse, ApiServiceError> {
        self.post::<_, Json<_>>(
            "auth/sessions/forks",
            PostAuthSessionsForksRequest {
                child_client_id: child_client_id.unwrap_or("web-account-lite".to_owned()),
                independent: 0,
            },
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `tfa_code` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn post_auth_tfa(
        &self,
        tfa_code: String,
    ) -> Result<PostAuthTfaResponse, ApiServiceError> {
        self.post::<_, Json<_>>(
            "auth/v4/2fa",
            PostAuthTfaRequest {
                two_factor_code: tfa_code,

                // FIXME: Why so much empty data?
                fido2: Fido2Auth {
                    authentication_data: String::new(),
                    authentication_options: JsonValue::Null,
                    client_data: String::new(),
                    credential_id: vec![],
                    signature: String::new(),
                },
            },
            None,
        )
        .await
    }

    /// Retrieve the API authentication store
    pub(crate) fn auth_store(&self) -> &AsyncRwLock<CachedStore> {
        self.auth.as_ref()
    }
}
