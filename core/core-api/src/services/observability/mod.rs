use super::proton::prelude::{PostMetricsRequestData, PostMetricsRequestElement};
use crate::{service::ApiServiceError, services::proton::ProtonData};
use std::{sync::Arc, time::Duration};

use chrono::Utc;
use muon::Client;
use serde::{Deserialize, Serialize};
use store::MetricStore;
use tokio::sync::RwLock;
use tracing::{Instrument, Span, debug, error, info, trace};

pub mod metrics;
pub mod store;

use crate::status_observer::StatusObserver;

pub trait ObservabilityMetric: Serialize + Send {
    const NAME: &str;
    const VERSION: u64;
}

#[derive(Clone)]
pub struct ObservabilityManager {
    client: Client,
    status: StatusObserver,
    store: Arc<RwLock<dyn MetricStore>>,
}

impl ObservabilityManager {
    /// Creates a new `ObservabilityManager` instance and starts its metric-sending task.
    ///
    /// This method initializes an `ObservabilityManager` with the provided `client` and `store`,
    /// then immediately starts a background task to periodically send metrics at the specified
    /// `send_period` with a defined `batch_size`. The manager is returned after the task is spawned.
    ///
    /// # Arguments
    /// * `client` - The `Client` instance used to send metrics.
    /// * `store` - An implementation of `MetricStore` that holds the metrics data. It must be
    ///   `'static` to ensure it can be safely stored and accessed across threads.
    /// * `send_period` - The `Duration` specifying how often metrics should be sent.
    /// * `batch_size` - The maximum number of metrics to send in each batch.
    ///
    /// # Returns
    /// Returns the initialized `ObservabilityManager` instance with its metric-sending task
    /// already running.
    #[must_use]
    pub fn create_and_start(
        client: Client,
        store: impl MetricStore + 'static,
        status: StatusObserver,
        send_period: Duration,
        batch_size: usize,
    ) -> Self {
        let manager = Self {
            client,
            status,
            store: Arc::new(RwLock::new(store)),
        };
        manager.start(send_period, batch_size);
        manager
    }

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
    pub fn start(&self, send_period: Duration, batch_size: usize) {
        let manager = self.clone();
        tokio::spawn(async move {
            info!("Start ObservabilityManager task");
            let mut interval = tokio::time::interval(send_period);
            loop {
                interval.tick().await;
                trace!("ObservabilityManager tick");
                manager.post_metrics(batch_size).await;
            }
        });
    }

    pub fn record<T: ObservabilityMetric>(&self, metric: T) {
        let element = match Self::into_metrics_element(metric) {
            Ok(element) => element,
            Err(err) => {
                error!("Could not serialize metric: {err:?}");
                return;
            }
        };
        self.store_element(element);
    }

    fn store_element(&self, element: PostMetricsRequestElement) {
        let store = Arc::clone(&self.store);
        tokio::spawn(
            async move {
                let mut store_lock = store.write().await;
                if let Err(err) = store_lock.store(element) {
                    error!("Could not store metric element: {err:?}");
                }
            }
            .instrument(Span::current()),
        );
    }

    async fn post_metrics(&self, count: usize) {
        let client = self.client.clone();
        let mut store_lock = self.store.write().await;
        match store_lock.get_first_n(count) {
            Ok(elements) => {
                let metric_count = elements.len();
                if metric_count == 0 {
                    trace!("No metrics to send");
                    return;
                }
                if self.status.status(self.client.clone()).await.is_offline() {
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
            }
            Err(err) => {
                error!("Error while loading metrics from store: {err:?}");
            }
        }
        if let Err(err) = store_lock.remove_first_n(count) {
            error!("Error while removing first {count} element from store: {err:?}");
        }
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
