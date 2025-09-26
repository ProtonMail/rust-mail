use std::collections::VecDeque;

use proton_core_api::services::proton::prelude::PostMetricsRequestElement;

const DEFAULT_STORE_CAPACITY: usize = 512;

/// Stores metrics in a fixed-capacity FIFO queue.
///
/// Removes the oldest element (from the back) if the capacity is reached, then adds the new element to the front.
#[derive(Debug)]
pub struct InMemoryMetricStore {
    metrics: VecDeque<PostMetricsRequestElement>,
    max_metrics: usize,
}

impl InMemoryMetricStore {
    /// Stores a metric.
    /// Removes the oldest element (from the front) if the capacity is reached, then adds the new element to the back.
    pub fn store(&mut self, metric: PostMetricsRequestElement) {
        if self.metrics.len() >= self.max_metrics {
            self.metrics.pop_front(); // Remove oldest element
        }
        self.metrics.push_back(metric);
    }

    /// Removes the first n elements (oldest first).
    #[must_use]
    pub fn remove_first_n(&mut self, count: usize) -> Vec<PostMetricsRequestElement> {
        self.metrics
            .drain(0..count.min(self.metrics.len()))
            .collect()
    }
}

impl InMemoryMetricStore {
    #[must_use]
    pub fn new(max_metrics: usize) -> Self {
        Self {
            metrics: VecDeque::with_capacity(max_metrics),
            max_metrics,
        }
    }
}

impl Default for InMemoryMetricStore {
    fn default() -> Self {
        Self::new(DEFAULT_STORE_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use proton_core_api::services::proton::prelude::PostMetricsRequestData;

    use super::*;

    #[test]
    fn test_inmemory_store_with_single_value() {
        let mut store = InMemoryMetricStore::default();
        let element = PostMetricsRequestElement {
            name: String::from("test"),
            version: 2,
            timestamp: 3,
            data: PostMetricsRequestData {
                labels: json!({"status": "http2xx"}),
                value: 1,
            },
        };
        store.store(element.clone());
        assert_eq!(store.metrics.len(), 1);
        assert_eq!(store.remove_first_n(1)[0], element);
    }

    #[test]
    fn test_inmemory_store_with_multiple_values() {
        let mut store = InMemoryMetricStore::default();
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
            store.store(element.clone());
        }
        assert_eq!(store.metrics.len(), 10);

        let batch = store.remove_first_n(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].version, 0);
        assert_eq!(batch[1].version, 1);
        assert_eq!(batch[2].version, 2);
    }

    #[test]
    fn test_inmemory_delete_with_multiple_values() {
        let mut store = InMemoryMetricStore::default();
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
            store.store(element.clone());
        }

        let _ = store.remove_first_n(3);
        assert_eq!(store.metrics.len(), 7);
        let batch = store.remove_first_n(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].version, 3);
        assert_eq!(batch[1].version, 4);
        assert_eq!(batch[2].version, 5);
    }

    #[test]
    fn test_inmemory_delete_more_values_than_max() {
        let mut store = InMemoryMetricStore::default();
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
            store.store(element.clone());
        }
        assert_eq!(store.metrics.len(), 10);

        let _ = store.remove_first_n(500);
        assert_eq!(store.metrics.len(), 0);
    }

    #[test]
    fn test_inmemory_insert_more_than_capacity_elements() {
        let mut store = InMemoryMetricStore::default();
        for _ in 0..(DEFAULT_STORE_CAPACITY * 2) {
            let element = PostMetricsRequestElement {
                name: String::from("test"),
                version: 1,
                timestamp: 33333,
                data: PostMetricsRequestData {
                    labels: json!({"status": "http2xx"}),
                    value: 1,
                },
            };
            store.store(element.clone());
        }
        assert_eq!(store.metrics.len(), DEFAULT_STORE_CAPACITY);
    }
}
