use crate::UserContext;
use proton_core_api::services::proton::ProtonData;
use proton_core_api::services::proton::prelude::{
    PostMetricsRequestData, PostMetricsRequestElement,
};
use std::{
    sync::{Arc, LazyLock, Once, Weak},
    time::Duration,
};

use chrono::Utc;
use parking_lot::RwLock;
use proton_network_monitor_service::NetworkStatusObserver;
use serde::Serialize;
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
    /// This method spawns an asynchronous task that runs indefinitely, sending metrics
    /// from the `MetricStore` using the `Client` at intervals defined by `send_period`. The
    /// number of metrics sent per tick is limited by `batch_size`. The task logs its start
    /// and each tick for debugging purposes.
    pub fn start(
        status: NetworkStatusObserver,
        user_context: &Weak<UserContext>,
        send_period: Duration,
        batch_size: usize,
    ) {
        START.call_once(|| {
            let Some(ctx) = user_context.upgrade() else {
                error!("User context already dropped, cannot start ObservabilityManager task");
                return;
            };

            let user_context_clone = user_context.clone();
            ctx.spawn(async move {
                info!("Start ObservabilityManager task");

                let mut interval = tokio::time::interval(send_period);

                loop {
                    interval.tick().await;
                    trace!("ObservabilityManager tick");

                    if user_context_clone.upgrade().is_none() {
                        debug!("User context dropped, exiting ObservabilityManager task");
                        break;
                    }

                    Self::post_metrics(batch_size, &status, &user_context_clone).await;
                }
            });
        });
    }

    /// Sends a batch of metrics to the remote endpoint.
    ///
    /// Retrieves up to `count` metrics from the store, checks if the client is
    /// online, and sends the metrics using the provided client. Removes sent
    /// metrics from the store upon success.
    async fn post_metrics(
        batch_size: usize,
        status: &NetworkStatusObserver,
        user_context: &Weak<UserContext>,
    ) {
        if !status.is_online() {
            trace!("Client is offline");
            return;
        }

        let Some(ctx) = user_context.upgrade() else {
            debug!("User context dropped, not sending pre-login metrics");
            return;
        };

        let telemetry_enabled = match ctx.user_settings().await {
            Ok(settings) => settings.telemetry,
            Err(err) => {
                debug!("Could not get user settings: {err:?}, not sending pre-login metrics");
                return;
            }
        };

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

        if !telemetry_enabled {
            debug!(
                "User has disabled telemetry, discarding {} pre-login metrics",
                metric_count
            );
            return; // Metrics already removed from store, just don't send
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

        let client = ctx.session();
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
