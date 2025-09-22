use proton_core_api::services::proton::prelude::{
    PostMetricsRequestData, PostMetricsRequestElement,
};
use std::sync::{Arc, LazyLock};

use chrono::Utc;
use parking_lot::Mutex;
use serde::Serialize;
use store::InMemoryMetricStore;
use tracing::{error, trace};

pub mod metrics;
pub mod store;

static PRE_LOGIN_METRIC_STORE: LazyLock<Arc<Mutex<InMemoryMetricStore>>> =
    LazyLock::new(|| Arc::new(Mutex::new(InMemoryMetricStore::default())));

pub trait ObservabilityMetric: Serialize {
    const NAME: &str;
    const VERSION: u64;
}

pub fn steal_from_pre_login_metric_store(batch_size: usize) -> Vec<PostMetricsRequestElement> {
    let Some(mut store_guard) = PRE_LOGIN_METRIC_STORE.try_lock() else {
        trace!("Pre-login metric store is busy, skipping steal attempt");
        return Vec::new();
    };

    store_guard.remove_first_n(batch_size)
}

#[derive(Clone, Debug, Default)]
pub struct PreLoginMetricRecorder {
    _priv: (),
}

impl PreLoginMetricRecorder {
    pub fn record<T: ObservabilityMetric>(&self, metric: T) {
        match into_metrics_element(metric, Utc::now().timestamp(), 1) {
            Ok(element) => {
                PRE_LOGIN_METRIC_STORE.lock().store(element);
            }
            Err(err) => {
                error!("Could not serialize metric: {err:?}");
            }
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
