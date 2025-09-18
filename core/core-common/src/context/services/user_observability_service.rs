use anyhow::anyhow;
use parking_lot::RwLock;
use std::sync::Weak;
use std::time::Duration;
use tracing::{debug, error, trace};

use super::Service;
use crate::observability::{
    ObservabilityMetric, ObservabilityRecorder, store::InMemoryMetricStore,
};
use crate::{CoreContextError, UserContext};
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::services::proton::ProtonData;

const OBSERVABILITY_SEND_INTERVAL_SECS: u64 = 60;
const OBSERVABILITY_BATCH_SIZE: usize = 500;

/// Per-account observability service that handles telemetry collection and transmission.
///
/// For events that happened before user logged in to any account, use [`ObservabilityRecorder`] directly.
/// For events with logged-in user account, use this service instead via `user_context.observability_service()`.
pub struct UserObservabilityService {
    ctx: Weak<UserContext>,
    store: RwLock<InMemoryMetricStore>,
    recorder: ObservabilityRecorder,
}

impl UserObservabilityService {
    #[must_use]
    pub fn new(ctx: Weak<UserContext>) -> Self {
        let store = RwLock::new(InMemoryMetricStore::default());
        let recorder = ObservabilityRecorder::default();

        Self {
            ctx,
            store,
            recorder,
        }
    }

    async fn send_metrics_if_enabled(ctx: &UserContext) {
        trace!("UserObservabilityService: Checking telemetry and sending metrics");

        let telemetry_enabled = match ctx.user_settings().await {
            Ok(settings) => settings.telemetry,
            Err(e) => {
                error!("Failed to get user settings before sending metrics: {e:?}");
                return;
            }
        };

        if !telemetry_enabled {
            trace!("Telemetry disabled for user, skipping metric send");
            return;
        }

        let Some(service) = ctx.get_service_opt::<UserObservabilityService>() else {
            error!("UserObservabilityService not found in context");
            return;
        };

        let connection_status = ctx.connection_status();

        if connection_status != ConnectionStatus::Online {
            trace!("Network offline, skipping metric send");
            return;
        }

        let elements = {
            service
                .store
                .write()
                .remove_first_n(OBSERVABILITY_BATCH_SIZE)
        };

        let metric_count = elements.len();
        if metric_count == 0 {
            trace!("No metrics to send");
            return;
        }

        debug!(
            "Sending {} metrics for user {}",
            metric_count,
            ctx.user_id()
        );

        let client = ctx.session();
        match client.post_metrics(elements).await {
            Ok(()) => {
                debug!("Successfully sent {} metrics", metric_count);
            }
            Err(err) => {
                error!("Error sending metrics: {err:?}");
            }
        }
    }

    pub async fn record_metric_if_enabled<T>(
        &self,
        metric: T,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        T: ObservabilityMetric,
    {
        let Some(ctx) = self.ctx.upgrade() else {
            trace!("Context dropped, not recording metric");
            return Ok(());
        };

        let telemetry_enabled = ctx.user_settings().await?.telemetry;

        self.recorder.record(metric, telemetry_enabled);
        Ok(())
    }

    pub fn record_metric_with_permission<T>(&self, metric: T, telemetry_enabled: bool)
    where
        T: ObservabilityMetric,
    {
        if telemetry_enabled {
            let element = match ObservabilityRecorder::into_metrics_element(
                metric,
                chrono::Utc::now().timestamp(),
                1,
            ) {
                Ok(element) => element,
                Err(err) => {
                    error!("Could not serialize metric: {err:?}");
                    return;
                }
            };

            self.store.write().store(element);
        }
    }
}

#[async_trait::async_trait]
impl Service for UserObservabilityService {
    type Error = CoreContextError;

    async fn init(&self) -> Result<(), Self::Error> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(CoreContextError::Other(anyhow!(
                "Could not upgrade UserContext"
            )));
        };

        let ctx_weak = self.ctx.clone();
        ctx.spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(OBSERVABILITY_SEND_INTERVAL_SECS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            debug!("UserObservabilityService background task started");

            loop {
                interval.tick().await;

                let Some(ctx) = ctx_weak.upgrade() else {
                    debug!("UserObservabilityService: Context dropped, exiting task");
                    return;
                };

                Self::send_metrics_if_enabled(&ctx).await;
                drop(ctx);
            }
        });

        Ok(())
    }
}
