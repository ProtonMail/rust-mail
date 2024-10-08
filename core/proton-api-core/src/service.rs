#![allow(async_fn_in_trait)]
#![allow(clippy::module_name_repetitions)]

use crate::auth::StoreError;
use bytes::Bytes;
use colored::Colorize;
use regex::Regex;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{
    header::HeaderMap, Client, Error as ReqwestError, Method, RequestBuilder, Response, StatusCode,
    Url,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Error as JsonError;
use serde_qs::to_string as to_query_string;
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::string::FromUtf8Error;
use thiserror::Error;
use tracing::error;

/// Syntactic sugar for when there are no query parameters, clearer and more
/// obvious than writing `None::<()>`.
pub const NO_PARAMS: Option<()> = None;

/// The possible errors that can occur when using an external API.
///
/// The possible errors fall into a few categories:
///
///   - Network level
///   - Protocol level
///   - Data level
///   - Logic level
///
/// ## Network level
///
/// If there are problems establishing a network connection to the external API,
/// or issues during the exchange of data, then these errors will be generated.
/// These are Reqwest errors, and are at a low level of interaction, and not at
/// a protocol level.
///
/// These errors are reported internally by this system, from the Reqwest
/// library.
///
/// ## Protocol level
///
/// HTTP errors will be relayed through as they are, and will not represent
/// anything other than there being a problem at a protocol level, with the
/// exchange of request and response. These errors could be caused by incorrect
/// data being sent to the external API, or by a problem with the external API.
/// Problems reported by the external API could be due to this system doing
/// something wrong, but equally could be due to the external API service doing
/// something wrong.
///
/// The errors that can occur here are related to bad requests, missing
/// resources, invalid data, and internal server errors — all from the
/// perspective of the external API service. These are all errors that are
/// reported back by the external API.
///
/// ## Data level
///
/// Errors of this nature are related to the data that is returned from the
/// external API. These errors are caused by the external API returning data
/// that is not in the expected format, or is missing required fields. For
/// whatever reason, deserialisation of the data has failed — so the response
/// was potentially received correctly, and was complete, but something about
/// the data meant it could not be processed.
///
/// Data errors refer here generally to all JSON deserialisation errors, which
/// are first and foremost schema validation errors, but could also be flagged
/// at an intra-field level. Structure, type, and format errors are all errors
/// of this nature.
///
/// ## Logic level
///
/// These errors are related to the logic of the system, and are not related to
/// the external API. These errors are caused by the system not being able to
/// carry out an expected task or function, despite having valid data.
///
#[derive(Debug, Error)]
pub enum ApiServiceError {
    //  NETWORK ERRORS
    //==========================================================================
    /// An internal Reqwest error has occurred, specifically when attempting to
    /// make a connection.
    #[error("Network connection error: {0}")]
    ConnectionError(String),

    /// An internal Reqwest error has occurred. This could be due to a network
    /// error, or a misconfiguration, causing the request to fail.
    #[error("Network error: {0}")]
    NetworkError(#[from] ReqwestError),

    /// An internal Reqwest error has occurred, specifically, we have been
    /// redirected.
    #[error("Redirect error for {0}: {1}")]
    Redirect(String, String),

    /// An internal Reqwest error has occurred, specifically, the HTTP request
    /// has timed out.
    #[error("Timeout: {0}")]
    Timeout(String),

    //  PROTOCOL ERRORS
    //==========================================================================
    /// 400: The request has been made incorrectly.
    #[error("Bad request: {0}. {1}")]
    BadRequest(String, String),

    /// 401: The request was rejected due to authentication failure.
    #[error("Unauthorized: {0}. {1}")]
    Unauthorized(String, String),

    /// 404: The URL requested on the external API was not found.
    #[error("Not found: {0}. {1}")]
    NotFound(String, String),

    /// 422: The data/request provided was invalid in terms or structure or
    /// contents, and could not be processed by the external API service.
    #[error("Unprocessable entity: {0}. {1}")]
    UnprocessableEntity(String, String),

    /// 429: The client made too many requests to the server.
    #[error("Too many requests: {0}. {1}")]
    TooManyRequest(String, String),

    /// 500: Something is wrong with the external API service.
    #[error("Internal server error: {0}. {1}")]
    InternalServerError(String, String),

    /// 501: The server either does not recognize the request method, or it lacks the ability to
    /// fulfil the request.
    #[error("Not Implemented: {0}. {1}")]
    NotImplemented(String, String),

    /// 502: The server was acting as a gateway or proxy and received an invalid response from the
    /// upstream server.
    #[error("Bad gateway: {0}. {1}")]
    BadGateway(String, String),

    /// 503: The server cannot handle the request (because it is overloaded or down for maintenance).
    #[error("Service Unavailable: {0}. {1}")]
    ServiceUnavailable(String, String),

    /// Any other HTTP error which is not currently handled.
    #[error("HTTP error {0}: {1}. {2}")]
    OtherHttpError(StatusCode, String, String),

    //  DATA ERRORS
    //==========================================================================
    /// There has been a failure in decoding the JSON data returned from the
    /// external API into the appropriate structs.
    #[error("JSON deserialization error: {0}, context: {1}")]
    JsonError(JsonError, String),

    /// There has been a failure in compositing the HTTP request to send. Note
    /// that this is not a network error, but an error in the request itself.
    #[error("Request composition error: {0}")]
    RequestError(String),

    /// There has been a failure in decoding the data returned from the external
    /// API into valid UTF8 text.
    #[error("UTF8 decoding error: {0}")]
    Utf8DecodingError(FromUtf8Error),

    //  LOGIC ERRORS
    //==========================================================================
    /// An error has been reported by the implementing service. We don't worry
    /// too much about use of `Box` or dynamic traits here, as performance is
    /// not critical in this context.
    #[error("API Service error: {0}")]
    ServiceError(Box<dyn ServiceError>),

    /// An unsupported HTTP method was specified.
    #[error("Unsupported HTTP method: {0}")]
    UnsupportedHttpMethod(Method),

    /// Authentication store operation failed.
    #[error("Authentication Store error: {0}")]
    AuthStore(#[from] StoreError),

    /// An unknown error has occurred. These should be monitored and specific
    /// handling added in.
    #[error("Unknown error: {0}")]
    UnknownError(String),
}

impl ApiServiceError {
    /// Check if the error is the result of a network failure.
    ///
    /// An error is considered a network failure the server replies with 429/5xx HTTP status codes
    /// or there was an issue with the underlying network transport layer.
    #[must_use]
    pub fn is_network_failure(&self) -> bool {
        match self {
            ApiServiceError::Redirect(_, _)
            | ApiServiceError::Timeout(_)
            | ApiServiceError::NetworkError(_)
            | ApiServiceError::ConnectionError(_)
            | ApiServiceError::TooManyRequest(_, _)
            | ApiServiceError::BadGateway(_, _)
            | ApiServiceError::NotImplemented(_, _)
            | ApiServiceError::ServiceUnavailable(_, _)
            | ApiServiceError::InternalServerError(_, _) => true,
            ApiServiceError::OtherHttpError(code, _, _) => code.as_u16() >= 500,
            _ => false,
        }
    }
}

/// Formalised list of the possible body types that can be sent with a request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Body<J>
where
    J: Clone + Serialize + Send + Sync,
{
    /// Raw binary data, such as an image or file.
    Bytes(Bytes),

    /// Form data, such as that sent by an HTML form.
    Form(HashMap<String, String>),

    /// JSON data, (de)serialisable to/from a struct.
    Json(J),

    /// UTF8-encoded text.
    String(String),

    /// No body data.
    #[default]
    None,
}

/// Wrapper for JSON data being returned as an API response.
pub struct Json<T>(T);

impl<T: DeserializeOwned> ApiResponse for Json<T> {
    type Inner = T;

    fn from_response(body: Bytes) -> Result<Self, ApiServiceError> {
        let text = String::from_utf8(body.to_vec()).map_err(|err| {
            error!("UTF-8 error: {:?}", err);
            ApiServiceError::Utf8DecodingError(err)
        })?;
        Ok(Json(serde_json::from_str::<T>(&text).map_err(|err| {
            if let Some((line, column)) = extract_line_column(&err.to_string()) {
                let error_snippet = extract_error_snippet(&text, line, column, 1000, 50);
                error!("JSON error: {:?}, context: {}", err, &error_snippet);
                ApiServiceError::JsonError(err, error_snippet)
            } else {
                error!("JSON error: {:?}, context: unknown", err);
                ApiServiceError::JsonError(
                    err,
                    "Unable to extract deserialization error context".to_owned(),
                )
            }
        })?))
    }

    fn into_inner(self) -> Self::Inner {
        self.0
    }
}

/// Formalisation of API requests.
#[derive(Clone, Debug, SmartDefault)]
pub struct Request<J = ()>
where
    J: Clone + Serialize + Send + Sync,
{
    /// The body of the request. This can be any of the types specified in the
    /// [`Body`] enum.
    pub body: Body<J>,

    /// Any headers to send with the request. These will get added to any
    /// persistent headers that have been set with [`set_header()`](ApiService::set_header()).
    pub headers: Option<HashMap<String, String>>,

    /// The HTTP method to use for the request.
    pub method: Method,

    /// The URL to send the request to.
    pub url: String,

    /// Phantom data for the request body. This is necessary because not all
    /// request types use JSON, and therefore won't specify the `J` generic.
    #[default(PhantomData)]
    pub _phantom: PhantomData<J>,
}

/// Functionality for communicating with an external HTTP-based API service.
pub trait ApiService {
    /// Gets the base URL for the API.
    fn base_url(&self) -> &Url;

    /// Obtains the inner Reqwest client object.
    fn client(&self) -> &Client;

    /// Combines the persistent headers with any specific headers for a request.
    ///
    /// # Parameters
    ///
    /// * `headers` - The specific headers to send with the request. These will
    ///               be added to any persistent headers that have been set with
    ///               [`set_persistent_header()`](ApiService::set_persistent_header()).
    ///
    fn combine_headers(&self, headers: Option<&HashMap<String, String>>) -> HeaderMap {
        let mut combined = self.headers();
        if let Some(extra_headers) = headers {
            for (name, value) in extra_headers {
                combined.insert(
                    HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    HeaderValue::from_bytes(value.as_bytes()).unwrap(),
                );
            }
        }
        combined
    }

    /// Sends a `DELETE` request to the specified URL.
    ///
    /// # Parameters
    ///
    /// * `endpoint` - The second half of the URL to send requests to. This will
    ///                be appended to the [`base_url()`](ApiService::base_url())
    ///                to form the full URL.
    /// * `params`   - The endpoint query string parameters.
    /// * `headers`  - Any headers to send specifically with this request. They
    ///                will be added to any persistent headers that have been
    ///                set with [`set_persistent_header()`](ApiService::set_persistent_header()).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn delete<Q, T>(
        &self,
        endpoint: &str,
        params: Option<Q>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        Q: Serialize,
        T: ApiResponse,
    {
        let query = params.and_then(|p| to_query_string(&p).ok());
        self.perform_request::<_, T>(Request::<()> {
            headers,
            method: Method::DELETE,
            url: self.get_url(endpoint, query.as_deref()),
            ..Default::default()
        })
        .await
    }

    /// Sends a `GET` request to the specified URL.
    ///
    /// # Parameters
    ///
    /// * `endpoint` - The second half of the URL to send requests to. This will
    ///                be appended to the [`base_url()`](ApiService::base_url())
    ///                to form the full URL.
    /// * `params`   - The endpoint query string parameters.
    /// * `headers`  - Any headers to send specifically with this request. They
    ///                will be added to any persistent headers that have been
    ///                set with [`set_persistent_header()`](ApiService::set_persistent_header()).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn get<Q, T>(
        &self,
        endpoint: &str,
        params: Option<Q>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        Q: Serialize + Debug,
        T: ApiResponse,
    {
        let query = params.and_then(|p| to_query_string(&p).ok());
        self.perform_request::<_, T>(Request::<()> {
            headers,
            url: self.get_url(endpoint, query.as_deref()),
            ..Default::default()
        })
        .await
    }

    /// Gets the base URL from [`base_url()`](self.base_url()), and adds path
    /// and query.
    ///
    /// # Parameters
    ///
    /// * `endpoint` - The request endpoint path.
    /// * `query`    - The URL query string.
    ///
    fn get_url(&self, endpoint: &str, query: Option<&str>) -> String {
        let mut url = self.base_url().clone().join(endpoint).unwrap();
        if let Some(q) = query {
            url.set_query(Some(q));
        }
        url.to_string()
    }

    /// Handles any non-HTTP errors that occur during the API service call.
    ///
    /// This function handles any errors that occur during the API service call,
    /// converting them into the appropriate [`ApiServiceError`] variant.
    ///
    /// # Parameters
    ///
    /// * `err` - The error that occurred.
    ///
    #[must_use]
    fn handle_error(error: ReqwestError) -> ApiServiceError {
        if error.is_timeout() {
            error!("Timeout: {:?}", error);
            ApiServiceError::Timeout(error.to_string())
        } else if error.is_connect() {
            error!("Network connection error: {:?}", error);
            ApiServiceError::ConnectionError(error.to_string())
        } else if error.is_redirect() {
            error!("Redirect: {:?}", error);
            ApiServiceError::Redirect(
                error
                    .url()
                    .map_or("Not specified".to_owned(), ToString::to_string),
                error.to_string(),
            )
        } else if error.is_request() {
            error!("Request composition error: {:?}", error);
            ApiServiceError::RequestError(error.to_string())
        } else {
            error!("Network error: {:?}", error);
            ApiServiceError::NetworkError(error)
        }
    }

    /// Handles any HTTP errors that occur during the API service call.
    ///
    /// This function handles any HTTP errors that occur during the API service
    /// call, converting them into the appropriate [`ApiServiceError`] variant.
    ///
    /// # Parameters
    ///
    /// * `err`      - The error that occurred.
    /// * `response` - The response object. This will be consumed in order to
    ///                extract any error message that the external API has
    ///                provided.
    ///
    async fn handle_http_error(error: ReqwestError, response: Response) -> ApiServiceError {
        if let Some(status) = error.status() {
            error!("HTTP error {}: {:?}", status, error);
            let text = match response.text().await {
                Ok(text) => text,
                Err(err) => {
                    error!("Network error: {:?}", err);
                    return ApiServiceError::NetworkError(err);
                }
            };
            match status {
                StatusCode::BAD_REQUEST => ApiServiceError::BadRequest(error.to_string(), text),
                StatusCode::UNAUTHORIZED => ApiServiceError::Unauthorized(error.to_string(), text),
                StatusCode::NOT_FOUND => ApiServiceError::NotFound(error.to_string(), text),
                StatusCode::UNPROCESSABLE_ENTITY => {
                    ApiServiceError::UnprocessableEntity(error.to_string(), text)
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    ApiServiceError::TooManyRequest(error.to_string(), text)
                }
                StatusCode::INTERNAL_SERVER_ERROR => {
                    ApiServiceError::InternalServerError(error.to_string(), text)
                }
                StatusCode::NOT_IMPLEMENTED => {
                    ApiServiceError::NotImplemented(error.to_string(), text)
                }
                StatusCode::BAD_GATEWAY => ApiServiceError::BadGateway(error.to_string(), text),
                StatusCode::SERVICE_UNAVAILABLE => {
                    ApiServiceError::ServiceUnavailable(error.to_string(), text)
                }
                _ => ApiServiceError::OtherHttpError(status, error.to_string(), text),
            }
        } else {
            error!("Network error: {:?}", error);
            ApiServiceError::NetworkError(error)
        }
    }

    /// Handles the response data.
    ///
    /// Handles the response data from a Reqwest request, and unifies the
    /// response as a type, or assigns a [`ApiServiceError`] dependent on the
    /// nature of the error, and the HTTP status code if applicable.
    ///
    /// # Parameters
    ///
    /// * `response` - The Reqwest response object.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialisation of the response data fails.
    ///
    async fn handle_response<T>(response: Response) -> Result<T::Inner, ApiServiceError>
    where
        T: ApiResponse,
    {
        let bytes = response.bytes().await.map_err(|err| {
            error!("Network error: {:?}", err);
            ApiServiceError::NetworkError(err)
        })?;
        Ok(T::from_response(bytes)
            .map_err(|err| {
                error!("Response handling error: {:?}", err);
                err
            })?
            .into_inner())
    }

    /// Gets any persistent client headers.
    ///
    /// These headers will be sent to the external API with every request.
    ///
    /// Note that this returns a clone of the headers.
    ///
    fn headers(&self) -> HeaderMap;

    /// Performs custom logic when an error occurs.
    ///
    /// This function allows the implementing service to further examine the
    /// baseline error produced, and transform it into a different error variant
    /// if necessary, or indeed perform additional actions such as retrying the
    /// request. This allows specific service implementations to handle any
    /// special context they may be aware of.
    ///
    /// # Parameters
    ///
    /// * `err`     - The error that occurred.
    /// * `request` - The request to send. This is a closure that returns a
    ///               Reqwest request builder. This is a function that can be
    ///               called multiple times, to allow for situations such as
    ///               retries.
    ///
    /// # Errors
    ///
    /// Returns an error by default, but this can be changed into a different
    /// error, or changed from error to success. Note that if changed to
    /// success, the return type must be the type `T` expected by the original
    /// caller.
    ///
    async fn on_error<J, T>(
        &self,
        error: ApiServiceError,
        request: Request<J>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: ApiResponse;

    /// Sends a `PATCH` request to the specified URL.
    ///
    /// # Parameters
    ///
    /// * `endpoint` - The second half of the URL to send requests to. This will
    ///                be appended to the [`base_url()`](ApiService::base_url())
    ///                to form the full URL.
    /// * `body`     - The request body to send.
    /// * `headers`  - Any headers to send specifically with this request. They
    ///                will be added to any persistent headers that have been
    ///                set with [`set_persistent_header()`](ApiService::set_persistent_header()).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn patch<J, T>(
        &self,
        endpoint: &str,
        body: J,
        headers: Option<HashMap<String, String>>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: ApiResponse,
    {
        self.perform_request::<J, T>(Request::<J> {
            body: Body::Json(body),
            headers,
            method: Method::PATCH,
            url: self.get_url(endpoint, None),
            ..Default::default()
        })
        .await
    }

    /// Performs a request and handles the response.
    ///
    /// This function is the core of the API service, and is responsible for
    /// sending the request, handling any errors that occur, and processing the
    /// response.
    ///
    /// It accepts a [`Request`] instance that contains the information needed
    /// to create and send the actual request. This allows for the request to be
    /// retried in the event of a failure.
    ///
    /// The process is split into multiple stages, all of which can be run
    /// individually if needed, and this method combines them all together.
    ///
    /// # Parameters
    ///
    /// * `request` - The details of the request to send. This is data that can
    ///               be used multiple times, to allow for situations such as
    ///               retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn perform_request<J, T>(&self, request: Request<J>) -> Result<T::Inner, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: ApiResponse,
    {
        match Self::send_request(self.prepare_request(&request)?).await {
            Ok(response) => Self::handle_response::<T>(response).await,
            Err(err) => self.on_error::<J, T>(err, request).await,
        }
    }

    /// Sends a `POST` request to the specified URL.
    ///
    /// # Parameters
    ///
    /// * `endpoint` - The second half of the URL to send requests to. This will
    ///                be appended to the [`base_url()`](ApiService::base_url())
    ///                to form the full URL.
    /// * `body`     - The request body to send.
    /// * `headers`  - Any headers to send specifically with this request. They
    ///                will be added to any persistent headers that have been
    ///                set with [`set_persistent_header()`](ApiService::set_persistent_header()).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn post<J, T>(
        &self,
        endpoint: &str,
        body: J,
        headers: Option<HashMap<String, String>>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: ApiResponse,
    {
        self.perform_request::<J, T>(Request::<J> {
            body: Body::Json(body),
            headers,
            method: Method::POST,
            url: self.get_url(endpoint, None),
            ..Default::default()
        })
        .await
    }

    /// Makes a `POST` request with a form.
    ///
    /// Sends a `POST` request with an `application/x-www-form-urlencoded` form.
    ///
    /// # Parameters
    ///
    /// * `endpoint` - The second half of the URL to send requests to. This will
    ///                be appended to the [`base_url()`](ApiService::base_url())
    ///                to form the full URL.
    /// * `body`     - The hashmap of the form data.
    /// * `headers`  - Any headers to send specifically with this request. They
    ///                will be added to any persistent headers that have been
    ///                set with [`set_persistent_header()`](ApiService::set_persistent_header()).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn post_form<T>(
        &self,
        endpoint: &str,
        body: HashMap<String, String>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        T: ApiResponse,
    {
        // .form(&body)
        self.perform_request::<_, T>(Request::<()> {
            body: Body::Form(body),
            headers,
            method: Method::POST,
            url: self.get_url(endpoint, None),
            ..Default::default()
        })
        .await
    }

    /// Prepares a request.
    ///
    /// This function is responsible for preparing a request, ready to send.
    ///
    /// It accepts a [`Request`] instance that contains the information needed
    /// to create and send the actual request.
    ///
    /// # Parameters
    ///
    /// * `request` - The details of the request to send. This is data that can
    ///               be used multiple times, to allow for situations such as
    ///               retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the specified HTTP method is not supported.
    ///
    fn prepare_request<J>(&self, request: &Request<J>) -> Result<RequestBuilder, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
    {
        let url = request.url.as_str();
        let builder = match request.method {
            Method::DELETE => self.client().delete(url),
            Method::GET => self.client().get(url),
            Method::PATCH => self.client().patch(url),
            Method::POST => self.client().post(url),
            Method::PUT => self.client().put(url),
            _ => {
                return Err(ApiServiceError::UnsupportedHttpMethod(
                    request.method.clone(),
                ))
            }
        }
        .headers(self.combine_headers(request.headers.as_ref()));
        // At present we clone the body for the Bytes and String types, which is not
        // efficient, but is needed for retry handling. This could potentially be
        // improved — the body() function does consume, but there may be ways around
        // it. For now it's not a great concern.
        // TODO: Improve handling of Bytes and String body types to not clone.
        Ok(match &request.body {
            Body::Bytes(data) => builder.body(data.clone()),
            Body::Form(data) => builder.form(data),
            Body::Json(data) => builder.json(data),
            Body::None => builder,
            Body::String(data) => builder.body(data.clone().to_string()),
        })
    }

    /// Sends a `PUT` request to the specified URL.
    ///
    /// # Parameters
    ///
    /// * `endpoint` - The second half of the URL to send requests to. This will
    ///                be appended to the [`base_url()`](ApiService::base_url())
    ///                to form the full URL.
    /// * `body`     - The request body to send.
    /// * `headers`  - Any headers to send specifically with this request. They
    ///                will be added to any persistent headers that have been
    ///                set with [`set_persistent_header()`](ApiService::set_persistent_header()).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn put<J, T>(
        &self,
        endpoint: &str,
        body: J,
        headers: Option<HashMap<String, String>>,
    ) -> Result<T::Inner, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: ApiResponse,
    {
        self.perform_request::<J, T>(Request::<J> {
            body: Body::Json(body),
            headers,
            method: Method::PUT,
            url: self.get_url(endpoint, None),
            ..Default::default()
        })
        .await
    }

    /// Retries a request and handles the response.
    ///
    /// This function is responsible for sending the request, handling any
    /// errors that occur, and processing the response.
    ///
    /// It accepts a [`Request`] instance that contains the information needed
    /// to create and send the actual request.
    ///
    /// It is identical to [`perform_request()`](ApiService::perform_request()),
    /// except it does not call [`on_error()`](ApiService::on_error()) if an error
    /// occurs, thereby avoiding any further retries.
    ///
    /// # Parameters
    ///
    /// * `request` - The details of the request to send. This is data that can
    ///               be used multiple times, to allow for situations such as
    ///               retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn retry_request<J, T>(&self, request: Request<J>) -> Result<T::Inner, ApiServiceError>
    where
        J: Clone + Serialize + Send + Sync,
        T: ApiResponse,
    {
        Self::handle_response::<T>(Self::send_request(self.prepare_request(&request)?).await?).await
    }

    /// Sends a request and handles any resulting errors.
    ///
    /// This function is responsible for sending the request and handling any
    /// errors that occur.
    ///
    /// # Parameters
    ///
    /// * `request` - The Reqwest request builder, prepared and ready to send.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn send_request(request: RequestBuilder) -> Result<Response, ApiServiceError> {
        match request.send().await {
            Ok(response) => {
                if let Err(err) = response.error_for_status_ref() {
                    Err(Self::handle_http_error(err, response).await)
                } else {
                    Ok(response)
                }
            }
            Err(err) => Err(Self::handle_error(err)),
        }
    }

    /// Allows for the setting of any persistent client headers.
    ///
    /// These headers will be sent to the external API with every request.
    ///
    /// # Parameters
    ///
    /// * `name`  - The header name.
    /// * `value` - The header value.
    ///
    fn set_header(&self, name: &str, value: &str);
}

/// Representation of the types of body that can be returned from the API.
pub trait ApiResponse: Sized {
    /// The inner type that the response body will be converted into.
    type Inner;

    /// Converts the response body into the appropriate type.
    ///
    /// # Parameters
    ///
    /// * `body` - The response body. This is always in bytes, and gets
    ///            converted as appropriate by the various implementations.
    ///
    /// # Errors
    ///
    /// Returns an error if the conversion fails.
    ///
    fn from_response(body: Bytes) -> Result<Self, ApiServiceError>;

    /// Provides access to the inner type.
    fn into_inner(self) -> Self::Inner;
}

impl ApiResponse for () {
    type Inner = ();

    fn from_response(_body: Bytes) -> Result<Self, ApiServiceError> {
        Ok(())
    }

    #[allow(clippy::semicolon_if_nothing_returned)]
    fn into_inner(self) -> Self::Inner {
        self
    }
}

impl ApiResponse for Bytes {
    type Inner = Bytes;

    fn from_response(body: Bytes) -> Result<Self, ApiServiceError> {
        Ok(body)
    }

    fn into_inner(self) -> Self::Inner {
        self
    }
}

impl ApiResponse for String {
    type Inner = String;

    fn from_response(body: Bytes) -> Result<Self, ApiServiceError> {
        String::from_utf8(body.to_vec()).map_err(|err| {
            error!("UTF-8 error: {:?}", err);
            ApiServiceError::Utf8DecodingError(err)
        })
    }

    fn into_inner(self) -> Self::Inner {
        self
    }
}

/// Marker trait for service errors.
pub trait ServiceError: Debug + Display + Send + Sync {}

/// Extracts the line and column number from a JSON deserialisation error.
///
/// Extracts the line and column number from a serde error message. Assumes that
/// the error message format includes "line X column Y".
///
/// # Parameters
///
/// * `msg` - The error message.
///
fn extract_line_column(msg: &str) -> Option<(usize, usize)> {
    let re = Regex::new(r"line (\d+) column (\d+)").unwrap();
    re.captures(msg).and_then(|caps| {
        caps.get(1).and_then(|line_match| {
            caps.get(2).and_then(|column_match| {
                line_match.as_str().parse::<usize>().ok().and_then(|line| {
                    column_match
                        .as_str()
                        .parse::<usize>()
                        .ok()
                        .map(|column| (line, column))
                })
            })
        })
    })
}

/// Extracts a snippet of the error location from the JSON response text.
///
/// Calculates the character index from a line and column, then extracts a
/// snippet of text to give insight into the deserialisation error. It will
/// add a marker to the error location: `<^>`.
///
/// # Parameters
///
/// * `text`        - The JSON response text.
/// * `line`        - The line number.
/// * `column`      - The column number.
/// * `cx_len_pre`  - The length of the context to extract.
/// * `cx_len_post` - The length of the context to extract.
///
fn extract_error_snippet(
    text: &str,
    line: usize,
    column: usize,
    cx_len_pre: usize,
    cx_len_post: usize,
) -> String {
    text.lines()
        .enumerate()
        .find_map(|(i, line_text)| {
            if i + 1 == line {
                let start = column.saturating_sub(cx_len_pre);
                let end = std::cmp::min(column + cx_len_post, line_text.len());
                return Some(format!(
                    "...{}{}{}...",
                    &line_text[start..column].cyan(),
                    "<MARK>".white().on_red(),
                    &line_text[column..end].cyan(),
                ));
            }
            None
        })
        .unwrap_or_else(|| "Error location not found in text.".to_owned())
}
