use anyhow::anyhow;
use parking_lot::RwLock;
use std::sync::Weak;
use std::time::Duration;
use tracing::{debug, error, trace};

use super::Service;
use crate::observability::{
    ObservabilityMetric, into_metrics_element, steal_from_pre_login_metric_store,
    store::InMemoryMetricStore,
};
use crate::{CoreContextError, UserContext};
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::services::proton::ProtonData;

const OBSERVABILITY_SEND_INTERVAL_SECS: u64 = 60;
const OBSERVABILITY_BATCH_SIZE: usize = 500;

pub struct UserMetricService {
    ctx: Weak<UserContext>,
    store: RwLock<InMemoryMetricStore>,
}

impl UserMetricService {
    #[must_use]
    pub fn new(ctx: Weak<UserContext>) -> Self {
        let store = RwLock::new(InMemoryMetricStore::default());

        Self { ctx, store }
    }

    async fn send_metrics_if_enabled(ctx: &UserContext) {
        trace!("UserMetricService: Checking telemetry and sending metrics");

        let telemetry_enabled = match ctx.user_settings().await {
            Ok(settings) => settings.telemetry,
            Err(e) => {
                error!("Failed to get user settings before sending metrics: {e:?}");
                return;
            }
        };

        if !telemetry_enabled {
            trace!("Telemetry disabled for user, skipping all metrics");
            return;
        }

        let connection_status = ctx.connection_status();
        if connection_status != ConnectionStatus::Online {
            trace!("Network offline, skipping all metrics");
            return;
        }

        let client = ctx.session();

        let pre_login_events = steal_from_pre_login_metric_store(OBSERVABILITY_BATCH_SIZE);
        if !pre_login_events.is_empty() {
            debug!(
                "Sending {} pre-login metrics for user {}",
                pre_login_events.len(),
                ctx.user_id()
            );
            match client.post_metrics(pre_login_events).await {
                Ok(()) => {
                    debug!("Successfully sent pre-login metrics");
                }
                Err(err) => {
                    error!("Error sending pre-login metrics: {err:?}");
                }
            }
        }

        let Some(service) = ctx.get_service_opt::<UserMetricService>() else {
            error!("UserMetricService not found in context");
            return;
        };

        let user_events = {
            service
                .store
                .write()
                .remove_first_n(OBSERVABILITY_BATCH_SIZE)
        };

        let user_metric_count = user_events.len();
        if user_metric_count == 0 {
            trace!("No user metrics to send");
            return;
        }

        debug!(
            "Sending {} user metrics for user {}",
            user_metric_count,
            ctx.user_id()
        );

        match client.post_metrics(user_events).await {
            Ok(()) => {
                debug!("Successfully sent {} user metrics", user_metric_count);
            }
            Err(err) => {
                error!("Error sending user metrics: {err:?}");
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

        if telemetry_enabled {
            let element = match into_metrics_element(metric, chrono::Utc::now().timestamp(), 1) {
                Ok(element) => element,
                Err(err) => {
                    error!("Could not serialize metric: {err:?}");
                    return Ok(());
                }
            };

            self.store.write().store(element);
        }
        Ok(())
    }

    pub fn record_metric_with_permission<T>(&self, metric: T, telemetry_enabled: bool)
    where
        T: ObservabilityMetric,
    {
        if telemetry_enabled {
            let element = match into_metrics_element(metric, chrono::Utc::now().timestamp(), 1) {
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
impl Service for UserMetricService {
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

            debug!("UserMetricService background task started");

            loop {
                interval.tick().await;

                let Some(ctx) = ctx_weak.upgrade() else {
                    debug!("UserMetricService: Context dropped, exiting task");
                    return;
                };

                Self::send_metrics_if_enabled(&ctx).await;
            }
        });

        Ok(())
    }
}
