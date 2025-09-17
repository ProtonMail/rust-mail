use super::proton::prelude::{PostMetricsRequestData, PostMetricsRequestElement};
use crate::{service::ApiServiceError, services::proton::ProtonData};
use std::{
    sync::{Arc, LazyLock, Once},
    time::Duration,
};

use chrono::Utc;
use muon::Client;
use parking_lot::RwLock;
use proton_network_monitor_service::NetworkStatusObserver;
use serde::{Deserialize, Serialize};
use store::InMemoryMetricStore;
use tracing::{debug, error, info, trace};

pub mod metrics;
pub mod store;

static START: Once = Once::new();

static MANAGER: LazyLock<Arc<ObservabilityManager>> = LazyLock::new(|| {
    Arc::new(ObservabilityManager {
        store: RwLock::new(InMemoryMetricStore::default()),
    })
});

pub trait ObservabilityMetric: Serialize {
    const NAME: &str;
    const VERSION: u64;
}

/// Global singleton observability manager.
///
/// This manager is for recording events that happened before user logged in to any account.
/// For events with logged-in user account, use `UserObservabilityService` instead via `user_context.observability_service()`.
#[derive(Debug)]
pub struct ObservabilityManager {
    store: RwLock<InMemoryMetricStore>,
}

impl ObservabilityManager {
    /// Starts a background task that periodically sends metrics at the specified interval and batch size.
    ///
    /// This method spawns an asynchronous Tokio task that runs indefinitely, sending metrics
    /// from the `MetricStore` using the `Client` at intervals defined by `send_period`. The
    /// number of metrics sent per tick is limited by `batch_size`. The task logs its start
    /// and each tick for debugging purposes.
    ///
    /// # Arguments
    /// * `send_period` - The `Duration` between each metric-sending operation.
    /// * `batch_size` - The maximum number of metrics to send in each batch.
    ///
    pub fn start(
        status: NetworkStatusObserver,
        client: Client,
        send_period: Duration,
        batch_size: usize,
    ) {
        START.call_once(|| {
            tokio::spawn(async move {
                info!("Start ObservabilityManager task");

                let mut interval = tokio::time::interval(send_period);

                loop {
                    interval.tick().await;
                    trace!("ObservabilityManager tick");
                    Self::post_metrics(batch_size, &client, &status).await;
                }
            });
        });
    }

    /// Sends a batch of metrics to the remote endpoint.
    ///
    /// Retrieves up to `count` metrics from the store, checks if the client is
    /// online, and sends the metrics using the provided client. Removes sent
    /// metrics from the store upon success.
    ///
    /// # Arguments
    ///
    /// * `count` - The maximum number of metrics to send.
    /// * `client` - The client used to send metrics.
    async fn post_metrics(batch_size: usize, client: &Client, status: &NetworkStatusObserver) {
        if !status.is_online() {
            trace!("Client is offline");
            return;
        }

        let elements = {
            // We intentionally drop metrics even on failure. If we break schema compatibility,
            // we prefer to continue sending newer, supported events rather than getting stuck
            // retrying outdated or malformed ones indefinitely.
            MANAGER.store.write().remove_first_n(batch_size)
        };

        let metric_count = elements.len();

        if metric_count == 0 {
            trace!("No metrics to send");
            return;
        }

        debug!("Preparing to send {} metric(s):", metric_count);
        for (i, metric) in elements.iter().enumerate() {
            debug!(
                "  [{}] name: '{}', version: {}, timestamp: {}, labels: {}",
                i + 1,
                metric.name,
                metric.version,
                metric.timestamp,
                metric.data.labels,
            );
        }

        match client.post_metrics(elements).await {
            Ok(()) => {
                debug!("{metric_count} Metric(s) has been sent");
            }
            Err(err) => {
                error!("Error while sending Observability Metrics: {err:?}");
            }
        }
    }
}

/// Records observability metrics to the global singleton store.
///
/// This recorder is for recording events that happened before user logged in to any account.
/// For events with logged-in user account, use `UserObservabilityService` instead via `user_context.observability_service()`.
#[derive(Clone, Debug, Default)]
pub struct ObservabilityRecorder {
    _priv: (),
}

impl ObservabilityRecorder {
    /// Records a metric to the observability system.
    ///
    /// Serializes the metric and stores it asynchronously in the manager's
    /// store. Errors during serialization or storage are logged.
    pub fn record<T: ObservabilityMetric>(&self, metric: T, should_record: bool) {
        if !should_record {
            return;
        }

        match Self::into_metrics_element(metric, Utc::now().timestamp(), 1) {
            Ok(element) => {
                MANAGER.store.write().store(element);
            }
            Err(err) => {
                error!("Could not serialize metric: {err:?}");
            }
        }
    }

    pub fn into_metrics_element<T>(
        metric: T,
        timestamp: i64,
        value: u64,
    ) -> Result<PostMetricsRequestElement, serde_json::Error>
    where
        T: ObservabilityMetric,
    {
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
            | ApiServiceError::Forbidden(..)
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
            | ApiServiceError::ParseEndpoint(..)
            | ApiServiceError::RequestError(..)
            | ApiServiceError::ResponseError(..) => {
                ApiServiceObservabilityResponse::SerializationError
            }
        }
    }
}
