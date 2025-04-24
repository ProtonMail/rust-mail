use super::proton::prelude::{PostMetricsRequestData, PostMetricsRequestElement};
use crate::{service::ApiServiceError, services::proton::ProtonData};
use std::{
    sync::{Arc, LazyLock, Once},
    time::Duration,
};

use chrono::Utc;
use muon::Client;
use serde::{Deserialize, Serialize};
use store::InMemoryMetricStore;
use tokio::sync::RwLock;
use tracing::{Instrument, Span, debug, error, info, trace};

pub mod metrics;
pub mod store;

use crate::status_observer::StatusObserver;

static START: Once = Once::new();

/// Global singleton for the observability manager, lazily initialized.
static MANAGER: LazyLock<Arc<RwLock<ObservabilityManager>>> = LazyLock::new(|| {
    Arc::new(RwLock::new(ObservabilityManager {
        status: StatusObserver::default(),
        store: Arc::new(RwLock::new(InMemoryMetricStore::default())),
    }))
});

pub trait ObservabilityMetric: Serialize {
    const NAME: &str;
    const VERSION: u64;
}

/// Manages the observability system, coordinating metric storage and sending.
///
/// This struct holds a `StatusObserver` for checking client connectivity and a
/// thread-safe `MetricStore` for storing metrics. It supports periodic metric
/// sending via an asynchronous task.
// #[derive(Clone)]
pub struct ObservabilityManager {
    status: StatusObserver,
    store: Arc<RwLock<InMemoryMetricStore>>,
}

/// Records metrics to the observability manager.
///
/// This struct provides a convenient interface for recording metrics, delegating
/// storage to the global [`ObservabilityManager`]. It uses a reference to the
/// singleton `MANAGER` for thread-safe operations.
pub struct ObservabilityRecorder {
    manager: Arc<RwLock<ObservabilityManager>>,
}

impl Default for ObservabilityRecorder {
    fn default() -> Self {
        Self {
            manager: Arc::clone(&MANAGER),
        }
    }
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
    /// # Panics
    /// This method does not panic directly, but the spawned task assumes that the Tokio runtime
    /// is available. If called outside a Tokio runtime, it will panic.
    /// ```
    pub fn start(client: Client, send_period: Duration, batch_size: usize) {
        START.call_once(|| {
            tokio::spawn(async move {
                info!("Start ObservabilityManager task");
                let mut interval = tokio::time::interval(send_period);
                loop {
                    interval.tick().await;
                    trace!("ObservabilityManager tick");
                    let mut manager = MANAGER.write().await;
                    manager.post_metrics(batch_size, &client).await;
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
    async fn post_metrics(&mut self, count: usize, client: &Client) {
        let mut store_lock = self.store.write().await;
        let elements = store_lock.get_first_n(count);
        let metric_count = elements.len();
        if metric_count == 0 {
            trace!("No metrics to send");
            return;
        }
        if self.status.status(client.clone()).await.is_offline() {
            trace!("Client is offline");
            return;
        }
        match client.post_metrics(elements).await {
            Ok(()) => {
                debug!("{metric_count} Metric(s) has been sent");
            }
            Err(err) => {
                error!("Error while sending Observability Metrics: {err:?}");
            }
        }
        store_lock.remove_first_n(count);
    }
}

impl ObservabilityRecorder {
    /// Records a metric to the observability system.
    ///
    /// Serializes the metric and stores it
    /// asynchronously in the manager's store. Errors during serialization or
    /// storage are logged.
    /// ```
    pub fn record<T: ObservabilityMetric>(&self, metric: T) {
        let element = match Self::into_metrics_element(metric, Utc::now().timestamp(), 1) {
            Ok(element) => element,
            Err(err) => {
                error!("Could not serialize metric: {err:?}");
                return;
            }
        };
        let manager = Arc::clone(&self.manager);
        tokio::spawn(
            async move {
                let manager_lock = manager.write().await;
                let mut store_lock = manager_lock.store.write().await;
                store_lock.store(element);
            }
            .instrument(Span::current()),
        );
    }

    pub fn into_metrics_element<T: ObservabilityMetric>(
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
            | ApiServiceError::ParseEndpoint(..)
            | ApiServiceError::RequestError(..)
            | ApiServiceError::ResponseError(..) => {
                ApiServiceObservabilityResponse::SerializationError
            }
        }
    }
}
