use super::proton::prelude::{PostMetricsRequestData, PostMetricsRequestElement};
use crate::{service::ApiServiceError, services::proton::ProtonData};
use chrono::Utc;
use muon::Client;
use serde::{Deserialize, Serialize};
use tracing::{Instrument, Span, debug, error};

pub mod metrics;

pub trait ObservabilityMetric: Serialize + Send {
    const NAME: &str;
    const VERSION: u64;
}

#[derive(Clone)]
pub struct ObservabilityManager {
    client: Client,
}

impl ObservabilityManager {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn record<T: ObservabilityMetric>(&self, metric: T) {
        let client = self.client.clone();
        let element = match Self::into_metrics_element(metric) {
            Ok(element) => element,
            Err(err) => {
                error!("Could not serialize metric: {err:?}");
                return;
            }
        };
        tokio::spawn(
            async move {
                match client.post_metrics(vec![element]).await {
                    Ok(()) => {
                        debug!("Metric has been sent {}", T::NAME);
                    }
                    Err(err) => {
                        error!("Error while sending Observability Metric: {err:?}");
                    }
                }
            }
            .instrument(Span::current()),
        );
    }

    fn into_metrics_element<T: ObservabilityMetric>(
        metric: T,
    ) -> Result<PostMetricsRequestElement, serde_json::Error> {
        Self::metrics_element(metric, Utc::now().timestamp(), 1)
    }

    pub fn metrics_element<T: ObservabilityMetric>(
        metric: T,
        timestamp: i64,
        value: u64,
    ) -> Result<PostMetricsRequestElement, serde_json::Error> {
        let labels = serde_json::to_value(metric)?;
        Ok(PostMetricsRequestElement {
            name: T::NAME.to_owned(),
            version: T::VERSION,
            timestamp,
            data: PostMetricsRequestData { labels, value },
        })
    }
}

#[derive(PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiServiceObservabilityResponse {
    Success,
    Http4xx,
    Http5xx,

    /// An internal muon error has occurred. This could be due to a network
    /// error, or a misconfiguration, causing the request to fail.
    NetworkError,

    /// There has been a failure in compositing the HTTP request/query parameters to send or
    /// the response received
    SerializationError,

    /// An unknown error has occurred. These should be monitored and specific
    /// handling added in.
    Unknown,
}

impl From<Option<&ApiServiceError>> for ApiServiceObservabilityResponse {
    fn from(value: Option<&ApiServiceError>) -> Self {
        match value {
            None => ApiServiceObservabilityResponse::Success,
            Some(err) => err.into(),
        }
    }
}

impl From<&ApiServiceError> for ApiServiceObservabilityResponse {
    fn from(value: &ApiServiceError) -> Self {
        match value {
            ApiServiceError::Timeout(..)
            | ApiServiceError::BadRequest(..)
            | ApiServiceError::Unauthorized(..)
            | ApiServiceError::NotFound(..)
            | ApiServiceError::UnprocessableEntity(..)
            | ApiServiceError::TooManyRequests(..) => ApiServiceObservabilityResponse::Http4xx,
            ApiServiceError::InternalServerError(..)
            | ApiServiceError::NotImplemented(..)
            | ApiServiceError::ServiceUnavailable(..)
            | ApiServiceError::BadGateway(..) => ApiServiceObservabilityResponse::Http5xx,
            ApiServiceError::OtherHttpError(..)
            | ApiServiceError::UnknownError(..)
            | ApiServiceError::AuthStore(..)
            | ApiServiceError::Redirect(_, _) => ApiServiceObservabilityResponse::Unknown,

            ApiServiceError::ConnectionError(..) | ApiServiceError::NetworkError(..) => {
                ApiServiceObservabilityResponse::NetworkError
            }

            ApiServiceError::Utf8DecodingError(..)
            | ApiServiceError::QueryStringError(..)
            | ApiServiceError::RequestError(..)
            | ApiServiceError::ResponseError(..) => {
                ApiServiceObservabilityResponse::SerializationError
            }
        }
    }
}
