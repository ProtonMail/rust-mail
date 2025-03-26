use crate::services::proton::prelude::PostMetricsRequestElement;

/// A trait defining the interface for storing and retrieving metrics.
///
/// Implementors of this trait must provide methods to store individual metrics, retrieve
/// a specified number of metrics, and remove a specified number of metrics from the store.
/// The trait requires `Send` and `Sync` bounds to ensure thread-safety, as it may be used
/// across threads in concurrent contexts.
pub trait MetricStore: Send + Sync {
    /// Stores a single metric in the store.
    ///
    /// # Arguments
    /// * `metric` - The `PostMetricsRequestElement` to store.
    ///
    /// # Returns
    /// Returns `Ok(())` if the metric was successfully stored, or an `anyhow::Error` if an
    /// error occurred during storage.
    fn store(&mut self, metric: PostMetricsRequestElement) -> Result<(), anyhow::Error>;

    /// Retrieves the first `n` metrics from the store.
    ///
    /// # Arguments
    /// * `count` - The maximum number of metrics to retrieve.
    ///
    /// # Returns
    /// Returns a `Result` containing a `Vec` of up to `count` `PostMetricsRequestElement`s,
    /// or an `anyhow::Error` if retrieval fails. If fewer than `count` metrics are available,
    /// returns all available metrics.
    fn get_first_n(&self, count: usize) -> Result<Vec<PostMetricsRequestElement>, anyhow::Error>;

    /// Removes the first `n` metrics from the store.
    ///
    /// # Arguments
    /// * `count` - The number of metrics to remove from the beginning of the store.
    ///
    /// # Returns
    /// Returns `Ok(())` if the metrics were successfully removed, or an `anyhow::Error` if
    /// an error occurred during removal. If fewer than `count` metrics are available, removes
    /// all available metrics.
    fn remove_first_n(&mut self, count: usize) -> Result<(), anyhow::Error>;
}

/// An in-memory implementation of the `MetricStore` trait.
///
/// This struct stores metrics in a `Vec`.
pub struct InMemoryMetricStore {
    metrics: Vec<PostMetricsRequestElement>,
}

impl MetricStore for InMemoryMetricStore {
    fn store(&mut self, metric: PostMetricsRequestElement) -> Result<(), anyhow::Error> {
        self.metrics.push(metric);
        Ok(())
    }

    fn get_first_n(&self, count: usize) -> Result<Vec<PostMetricsRequestElement>, anyhow::Error> {
        Ok(self.metrics.iter().take(count).cloned().collect())
    }

    fn remove_first_n(&mut self, count: usize) -> Result<(), anyhow::Error> {
        self.metrics.drain(0..count.min(self.metrics.len()));
        Ok(())
    }
}

impl InMemoryMetricStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            metrics: Vec::with_capacity(128),
        }
    }
}

impl Default for InMemoryMetricStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::services::proton::prelude::PostMetricsRequestData;

    use super::*;

    #[test]
    fn test_inmemory_store_with_single_value() {
        let mut store = InMemoryMetricStore::new();
        let element = PostMetricsRequestElement {
            name: String::from("test"),
            version: 2,
            timestamp: 3,
            data: PostMetricsRequestData {
                labels: json!({"status": "http2xx"}),
                value: 1,
            },
        };
        store.store(element.clone()).unwrap();
        assert_eq!(store.metrics.len(), 1);
        assert_eq!(store.get_first_n(1).unwrap().len(), 1);
        assert_eq!(store.get_first_n(1).unwrap()[0], element);
    }

    #[test]
    fn test_inmemory_store_with_multiple_values() {
        let mut store = InMemoryMetricStore::new();
        for i in 0..10 {
            let element = PostMetricsRequestElement {
                name: String::from("test"),
                version: i,
                timestamp: 33333,
                data: PostMetricsRequestData {
                    labels: json!({"status": "http2xx"}),
                    value: 1,
                },
            };
            store.store(element.clone()).unwrap();
        }
        assert_eq!(store.metrics.len(), 10);

        let batch = store.get_first_n(3).unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].version, 0);
        assert_eq!(batch[1].version, 1);
        assert_eq!(batch[2].version, 2);
    }

    #[test]
    fn test_inmemory_delete_with_multiple_values() {
        let mut store = InMemoryMetricStore::new();
        for i in 0..10 {
            let element = PostMetricsRequestElement {
                name: String::from("test"),
                version: i,
                timestamp: 33333,
                data: PostMetricsRequestData {
                    labels: json!({"status": "http2xx"}),
                    value: 1,
                },
            };
            store.store(element.clone()).unwrap();
        }

        store.remove_first_n(3).unwrap();
        assert_eq!(store.metrics.len(), 7);
        let batch = store.get_first_n(3).unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].version, 3);
        assert_eq!(batch[1].version, 4);
        assert_eq!(batch[2].version, 5);
    }

    #[test]
    fn test_inmemory_delete_more_values_than_max() {
        let mut store = InMemoryMetricStore::new();
        for i in 0..10 {
            let element = PostMetricsRequestElement {
                name: String::from("test"),
                version: i,
                timestamp: 33333,
                data: PostMetricsRequestData {
                    labels: json!({"status": "http2xx"}),
                    value: 1,
                },
            };
            store.store(element.clone()).unwrap();
        }
        assert_eq!(store.metrics.len(), 10);

        store.remove_first_n(500).unwrap();
        assert_eq!(store.metrics.len(), 0);
    }
}
