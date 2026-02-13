use anyhow::anyhow;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Weak;
use std::time::Duration;
use tracing::{debug, error, trace, warn};

use super::Service;
use crate::datatypes::{
    MeasurementData, MeasurementEventType, MeasurementValue, UnixTimestamp, UnixTimestampMs,
};
use crate::models::{Measurement, ModelExtension};
use crate::{CoreContextError, UserContext};
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::services::proton::measurements::requests::{
    PostMeasurementEventRequest, PostMeasurementEventsRequest,
};
use proton_core_api::services::proton::{
    MeasurementEventType as ApiMeasurementEventType, MeasurementValue as ApiMeasurementValue,
    ProtonMeasurements,
};
use stash::AccountDb;
use stash::orm::Model;
use stash::stash::{Stash, StashError};

const MEASUREMENT_SEND_INTERVAL_SECS: u64 = 60;
const MEASUREMENT_BATCH_SIZE: usize = 100;
const MEASUREMENT_FF_NAME: &str = "MailAndroidV7Events";

#[derive(Debug, Clone)]
struct MeasurementMetadata {
    asid: String,
    app_package_name: String,
}

pub struct MeasurementService {
    ctx: Weak<UserContext>,
    session_start_ms: RwLock<Option<u128>>,
    last_telemetry_state: RwLock<Option<bool>>,
    last_metadata: RwLock<Option<MeasurementMetadata>>,
}

impl MeasurementService {
    #[must_use]
    pub fn new(ctx: Weak<UserContext>) -> Self {
        Self {
            ctx,
            session_start_ms: RwLock::new(None),
            last_telemetry_state: RwLock::new(None),
            last_metadata: RwLock::new(None),
        }
    }

    pub fn clear_session_start(&self) {
        *self.session_start_ms.write() = None;
    }

    pub async fn record_prelogin(
        account_stash: &Stash<AccountDb>,
        event_type: MeasurementEventType,
        asid: String,
        app_package_name: String,
        fields: HashMap<String, Option<MeasurementValue>>,
    ) -> Result<(), StashError> {
        let measurement_data = MeasurementData {
            event_type,
            event_timestamp_ms: UnixTimestampMs::now(),
            asid,
            app_package_name,
            fields,
        };

        let mut measurement = Measurement {
            local_id: None,
            data: measurement_data,
            created_at: UnixTimestamp::now(),
        };

        let mut tether = account_stash.connection().await?;
        tether.tx(async |tx| measurement.save(tx).await).await?;

        Ok(())
    }

    async fn is_kill_switch_enabled(ctx: &UserContext) -> bool {
        ctx.global_feature_flags()
            .get(MEASUREMENT_FF_NAME)
            .await
            .inspect_err(|e| warn!("Could not fetch killswitch: {e}. Assuming its on"))
            .unwrap_or(Some(true))
            .unwrap_or(false)
    }

    pub async fn record(
        &self,
        event_type: MeasurementEventType,
        asid: String,
        app_package_name: String,
        fields: HashMap<String, Option<MeasurementValue>>,
    ) -> Result<(), CoreContextError> {
        let Some(ctx) = self.ctx.upgrade() else {
            trace!("Context dropped, not recording measurement");
            return Ok(());
        };

        if Self::is_kill_switch_enabled(&ctx).await {
            trace!("Kill switch enabled, not recording measurement");
            return Ok(());
        }

        let telemetry_enabled = ctx.user_settings().await?.telemetry;
        if !telemetry_enabled {
            trace!("Telemetry disabled, not recording measurement");
            return Ok(());
        }

        let measurement_data = MeasurementData {
            event_type,
            event_timestamp_ms: UnixTimestampMs::now(),
            asid: asid.clone(),
            app_package_name: app_package_name.clone(),
            fields,
        };

        *self.last_metadata.write() = Some(MeasurementMetadata {
            asid,
            app_package_name,
        });

        let mut measurement = Measurement {
            local_id: None,
            data: measurement_data,
            created_at: UnixTimestamp::now(),
        };

        let mut tether = ctx.account_stash().connection().await?;
        tether.tx(async |tx| measurement.save(tx).await).await?;

        Ok(())
    }

    async fn send_single_batch(
        ctx: &UserContext,
        service: &MeasurementService,
        events: PostMeasurementEventsRequest,
    ) -> anyhow::Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        debug!(
            "Sending {} measurement events for user {}",
            events.len(),
            ctx.user_id()
        );

        let client = ctx.session();
        let response = client.post_events(events).await?;

        *service.session_start_ms.write() = response.session_start_ms;

        Ok(())
    }

    fn get_last_measurement_metadata(&self) -> Option<MeasurementMetadata> {
        self.last_metadata.read().clone()
    }

    async fn handle_optout(ctx: &UserContext, service: &MeasurementService) -> anyhow::Result<()> {
        if let Some(metadata) = service.get_last_measurement_metadata() {
            let session_start_ms = *service.session_start_ms.read();

            let opt_out_event = PostMeasurementEventRequest {
                event_type: ApiMeasurementEventType::OptOut,
                event_timestamp_ms: UnixTimestampMs::now().as_u128(),
                asid: metadata.asid,
                app_package_name: metadata.app_package_name,
                session_start_ms,
                fields: HashMap::default(),
            };

            Self::send_single_batch(ctx, service, vec![opt_out_event]).await?;
        } else {
            warn!("No measurement metadata available, skipping OptOut event");
        }

        Ok(())
    }

    fn build_events_from_measurements(
        measurements: Vec<Measurement>,
        session_start_ms: Option<u128>,
    ) -> PostMeasurementEventsRequest {
        measurements
            .into_iter()
            .map(|m| {
                let fields = m
                    .data
                    .fields
                    .into_iter()
                    .map(|(k, v)| {
                        let api_value: Option<ApiMeasurementValue> = v.map(Into::into);
                        (k, api_value)
                    })
                    .collect();

                PostMeasurementEventRequest {
                    event_type: m.data.event_type.into(),
                    event_timestamp_ms: m.data.event_timestamp_ms.as_u128(),
                    asid: m.data.asid,
                    app_package_name: m.data.app_package_name,
                    session_start_ms,
                    fields,
                }
            })
            .collect()
    }

    async fn fetch_and_send_measurements(
        ctx: &UserContext,
        service: &MeasurementService,
    ) -> anyhow::Result<()> {
        let measurements = {
            let tether = ctx.account_stash().connection().await?;
            Measurement::fetch_batch(MEASUREMENT_BATCH_SIZE, &tether).await?
        };

        if measurements.is_empty() {
            trace!("No measurements to send");
            return Ok(());
        }

        debug!(
            "Preparing to send {} measurements for user {}",
            measurements.len(),
            ctx.user_id()
        );

        if let Some(last_measurement) = measurements.last() {
            *service.last_metadata.write() = Some(MeasurementMetadata {
                asid: last_measurement.data.asid.clone(),
                app_package_name: last_measurement.data.app_package_name.clone(),
            });
        }

        let session_start_ms = *service.session_start_ms.read();
        let measurement_ids = measurements
            .iter()
            .filter_map(|measurement| measurement.local_id)
            .collect::<Vec<_>>();
        let events = Self::build_events_from_measurements(measurements, session_start_ms);

        Self::send_single_batch(ctx, service, events).await?;

        let mut tether = ctx.account_stash().connection().await?;
        tether
            .tx(async |tx| Measurement::delete_by_ids(measurement_ids, tx).await)
            .await?;

        Ok(())
    }

    async fn send_measurements(
        ctx: &UserContext,
        service: &MeasurementService,
    ) -> anyhow::Result<()> {
        trace!("MeasurementService: Checking conditions and sending measurements");

        if Self::is_kill_switch_enabled(ctx).await {
            trace!("Kill switch enabled, skipping measurement sending");
            return Ok(());
        }

        let telemetry_enabled = ctx.user_settings().await?.telemetry;

        let previous_telemetry_state = {
            let mut last_state = service.last_telemetry_state.write();
            let previous = *last_state;
            *last_state = Some(telemetry_enabled);
            previous
        };

        if previous_telemetry_state == Some(true) && !telemetry_enabled {
            debug!("Telemetry changed from enabled to disabled, sending OptOut event");
            Self::handle_optout(ctx, service).await?;
        }

        if !telemetry_enabled {
            trace!("Telemetry disabled for user, clearing measurements");
            let mut tether = ctx.account_stash().connection().await?;
            tether
                .tx(async |tx| Measurement::delete_all(tx).await)
                .await?;

            return Ok(());
        }

        let connection_status = ctx.connection_status();
        if connection_status != ConnectionStatus::Online {
            trace!("Network offline, skipping measurements");
            return Ok(());
        }

        Self::fetch_and_send_measurements(ctx, service).await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Service for MeasurementService {
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
                tokio::time::interval(Duration::from_secs(MEASUREMENT_SEND_INTERVAL_SECS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            debug!("MeasurementService background task started");

            loop {
                interval.tick().await;

                let Some(ctx) = ctx_weak.upgrade() else {
                    debug!("MeasurementService: Context dropped, exiting task");
                    return;
                };

                let Some(service) = ctx.get_service_opt::<MeasurementService>() else {
                    error!("MeasurementService not found in context");
                    return;
                };

                if let Err(err) = Self::send_measurements(&ctx, service).await {
                    error!("Error sending measurements: {err:?}");
                }
            }
        });

        Ok(())
    }
}
