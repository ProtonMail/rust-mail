#![allow(async_fn_in_trait)]
#![allow(clippy::module_name_repetitions)]

use colored::Colorize;
use regex::Regex;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{
    header::HeaderMap, Client, Error as ReqwestError, RequestBuilder, Response, StatusCode, Url,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Error as JsonError;
use serde_urlencoded::to_string as to_query_string;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
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

    /// 500: Something is wrong with the external API service.
    #[error("Internal server error: {0}. {1}")]
    InternalServerError(String, String),

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

    //  LOGIC ERRORS
    //==========================================================================
    /// An error has been reported by the implementing service. We don't worry
    /// too much about use of `Box` or dynamic traits here, as performance is
    /// not critical in this context.
    #[error("API Service error: {0}")]
    ServiceError(Box<dyn ServiceError>),

    /// An unknown error has occurred. These should be monitored and specific
    /// handling added in.
    #[error("Unknown error: {0}")]
    UnknownError(String),
}

/// Functionality for communicating with an external HTTP-based API service.
pub trait ApiService {
    /// Generates a new external API service handler.
    ///
    /// # Parameters
    ///
    /// * `base_url` - The API base URL.
    /// * `headers`  - The headers to send with every request.
    ///
    fn new(base_url: Url, headers: Option<HeaderMap>) -> Self;

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
    fn combine_headers(&self, headers: Option<&HashMap<&str, &str>>) -> HeaderMap {
        let mut combined = self.headers().clone();
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
        headers: Option<&HashMap<&str, &str>>,
    ) -> Result<T, ApiServiceError>
    where
        Q: Serialize,
        T: DeserializeOwned,
    {
        let query = params.and_then(|p| to_query_string(p).ok());
        let url = self.get_url(endpoint, query.as_deref());
        let headers = self.combine_headers(headers);
        let client = self.client();
        self.execute_request::<T>(|| client.delete(url.as_str()).headers(headers.clone()))
            .await
    }

    /// Executes a request and handles the response.
    ///
    /// This function is the core of the API service, and is responsible for
    /// sending the request, handling any errors that occur, and processing the
    /// response.
    ///
    /// # Parameters
    ///
    /// * `request` - The request to send. This is a closure that returns a
    ///               Reqwest request builder. This is a function that can be
    ///               called multiple times, to allow for situations such as
    ///               retries.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, or if the response indicates
    /// failure.
    ///
    async fn execute_request<T>(
        &self,
        request: impl Fn() -> RequestBuilder + Send,
    ) -> Result<T, ApiServiceError>
    where
        T: DeserializeOwned,
    {
        let result = match request().send().await {
            Ok(response) => {
                if let Err(err) = response.error_for_status_ref() {
                    Err(Self::handle_http_error(err, response).await)
                } else {
                    Ok(response)
                }
            }
            Err(err) => Err(Self::handle_error(err)),
        };
        match result {
            Ok(response) => self.handle_response::<T>(response).await,
            Err(err) => Self::on_error::<T>(err, request).await,
        }
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
        headers: Option<&HashMap<&str, &str>>,
    ) -> Result<T, ApiServiceError>
    where
        Q: Serialize,
        T: DeserializeOwned,
    {
        let query = params.and_then(|p| to_query_string(p).ok());
        let url = self.get_url(endpoint, query.as_deref());
        let headers = self.combine_headers(headers);
        let client = self.client();
        self.execute_request::<T>(|| client.get(url.as_str()).headers(headers.clone()))
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
        let mut url = self.base_url().clone();
        url.set_path(endpoint);
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
                StatusCode::INTERNAL_SERVER_ERROR => {
                    ApiServiceError::InternalServerError(error.to_string(), text)
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
    async fn handle_response<T>(&self, response: Response) -> Result<T, ApiServiceError>
    where
        T: DeserializeOwned,
    {
        let text = response.text().await.map_err(|err| {
            error!("Network error: {:?}", err);
            ApiServiceError::NetworkError(err)
        })?;
        serde_json::from_str::<T>(&text).map_err(|err| {
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
        })
    }

    /// Gets any persistent client headers.
    ///
    /// These headers will be sent to the external API with every request.
    ///
    fn headers(&self) -> &HeaderMap;

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
    async fn on_error<T>(
        error: ApiServiceError,
        request: impl Fn() -> RequestBuilder + Send,
    ) -> Result<T, ApiServiceError>
    where
        T: DeserializeOwned;

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
    async fn patch<B, T>(
        &self,
        path: &str,
        body: B,
        headers: Option<&HashMap<&str, &str>>,
    ) -> Result<T, ApiServiceError>
    where
        B: Serialize + Send + Sync,
        T: DeserializeOwned,
    {
        let url = self.get_url(path, None);
        let headers = self.combine_headers(headers);
        let client = self.client();
        self.execute_request::<T>(|| {
            client
                .patch(url.as_str())
                .headers(headers.clone())
                .json(&body)
        })
        .await
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
    async fn post<B, T>(
        &self,
        path: &str,
        body: B,
        headers: Option<&HashMap<&str, &str>>,
    ) -> Result<T, ApiServiceError>
    where
        B: Serialize + Send + Sync,
        T: DeserializeOwned,
    {
        let url = self.get_url(path, None);
        let headers = self.combine_headers(headers);
        let client = self.client();
        self.execute_request::<T>(|| {
            client
                .post(url.as_str())
                .headers(headers.clone())
                .json(&body)
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
        body: HashMap<&str, String>,
        headers: Option<&HashMap<&str, &str>>,
    ) -> Result<T, ApiServiceError>
    where
        T: DeserializeOwned,
    {
        let url = self.get_url(endpoint, None);
        let headers = self.combine_headers(headers);
        let client = self.client();
        self.execute_request::<T>(|| {
            client
                .post(url.as_str())
                .headers(headers.clone())
                .form(&body)
        })
        .await
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
    async fn put<B, T>(
        &self,
        path: &str,
        body: B,
        headers: Option<&HashMap<&str, &str>>,
    ) -> Result<T, ApiServiceError>
    where
        B: Serialize + Send + Sync,
        T: DeserializeOwned,
    {
        let url = self.get_url(path, None);
        let headers = self.combine_headers(headers);
        let client = self.client();
        self.execute_request::<T>(|| {
            client
                .put(url.as_str())
                .headers(headers.clone())
                .json(&body)
        })
        .await
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
    fn set_header(&mut self, name: &str, value: &str);
}

/// Marker trait for service errors.
pub trait ServiceError: Debug + Display {}

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
