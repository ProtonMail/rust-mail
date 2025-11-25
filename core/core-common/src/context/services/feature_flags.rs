use std::collections::BTreeMap;
use std::sync::Weak;
use std::time::{Duration, Instant};

use crate::app_events::OnEnterForegroundEvent;
use crate::datatypes::UnixTimestamp;
use crate::models::{FeatureFlag, ModelExtension};
use crate::{Context, services::Service};
use crate::{CoreContextError, CoreContextResult};
use anyhow::{Context as _, Result};
use proton_core_api::services::proton::ProtonCore as _;
use proton_core_api::services::proton::muon::common::WithTimeout;
use proton_core_api::session::Session;

use stash::stash::WatcherHandle;
use stash::watcher::TableWatcher;
use tracing::error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureFlagsBackgroundTask {
    Enabled,
    Disabled,
}

#[derive(Clone)]
pub struct FeatureFlagsService {
    ctx: Weak<Context>,
    background_task_setting: FeatureFlagsBackgroundTask,
}

impl FeatureFlagsService {
    #[must_use]
    pub fn new(ctx: Weak<Context>, background_task_setting: FeatureFlagsBackgroundTask) -> Self {
        Self {
            ctx,
            background_task_setting,
        }
    }

    #[tracing::instrument(skip_all, name = "FeatureFlagsFetchAndUpdate")]
    async fn fetch_and_update(&self, api: &Session) -> CoreContextResult<()> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;

        let response = api.get_unleash_feature_flags().await?;
        tracing::info!("Fetched {} featured flags from API", response.toggles.len());

        let mut tether = ctx.account_stash().connection().await?;

        let mut flags = FeatureFlag::all(&tether)
            .await
            .inspect_err(|err| tracing::warn!("Failed to fetch feature flags: {}", err))
            .unwrap_or_default()
            .into_iter()
            .map(|flag| {
                (
                    flag.name.clone(),
                    FeatureFlag {
                        // If the flag is not fetched from API but exists in the database,
                        // we mark it as disabled.
                        enabled: false,
                        ..flag
                    },
                )
            })
            .collect::<BTreeMap<String, FeatureFlag>>();

        let modify_time = UnixTimestamp::now();

        for toggle in response.toggles {
            let flag = flags
                .entry(toggle.name.clone())
                .or_insert_with(|| FeatureFlag {
                    name: toggle.name,
                    enabled: false,
                    modify_time,
                });

            // Currently we are ignoring variants,
            // and Unleash API says that feature is always enabled
            flag.enabled = true;
            flag.modify_time = modify_time;
        }

        let flags = flags.into_values().collect();

        tether
            .tx(async |tx| FeatureFlag::save_all(flags, tx).await)
            .await?;

        Ok(())
    }

    pub async fn get(&self, key: &str) -> CoreContextResult<Option<bool>> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let feature_flag = {
            let tether = ctx.account_stash().connection().await?;
            FeatureFlag::by_name(key, &tether).await?
        };
        Ok(feature_flag.map(|flag| flag.enabled))
    }

    pub async fn refresh(&self) -> CoreContextResult<()> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let session = ctx.new_api_session(None).await?;
        self.fetch_and_update(&session).await
    }

    pub async fn list_all(&self) -> Vec<(String, bool)> {
        let Some(ctx) = self.ctx.upgrade() else {
            tracing::warn!("Failed to upgrade context");
            return vec![];
        };
        let Ok(tether) = ctx.account_stash().connection().await else {
            tracing::warn!("Failed to connect to account stash");
            return vec![];
        };
        let flags = FeatureFlag::all(&tether)
            .await
            .inspect_err(|err| tracing::warn!("Failed to fetch feature flags: {}", err))
            .unwrap_or_default();

        tracing::info!("Retrieved {} feature flags", flags.len());

        flags
            .iter()
            .map(|flag| (flag.name.clone(), flag.enabled))
            .collect()
    }

    pub async fn watch(&self) -> CoreContextResult<WatcherHandle> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;

        let stash = ctx.account_stash();
        TableWatcher::<FeatureFlag>::watch(stash)
            .await
            .map_err(CoreContextError::from)
    }
}

const REFRESH_THROTTLE_SECS: u64 = 60; // 1 minute
const REFRESH_TIMEOUT_SECS: u64 = 600; // 10 minutes

#[async_trait::async_trait]
impl Service for FeatureFlagsService {
    type Error = CoreContextError;

    async fn init(&self) -> Result<(), Self::Error> {
        if self.background_task_setting == FeatureFlagsBackgroundTask::Disabled {
            tracing::warn!("Feature flags background task is disabled");
            return Ok(());
        }
        let ctx = self
            .ctx
            .upgrade()
            .expect("Context to be there during initialization");

        let task_service = ctx.task_service();
        let event_service = ctx.event_service();
        let self_clone = self.clone();
        let Some(mut event_stream) = event_service.subscribe::<OnEnterForegroundEvent>() else {
            error!("Failed to subscribe to OnEnterForegroundEvent");
            return Ok(());
        };
        task_service.spawn(async move {
            let ctx = self_clone.ctx.upgrade().expect("Could not upgrade context");
            let Ok(session) = ctx.new_api_session(None).await else {
                error!("Failed to create API session");
                return;
            };
            drop(ctx);
            loop {
                if let Err(error) = self_clone.fetch_and_update(&session).await {
                    error!(%error, "Failed to refresh feature flags");
                }
                let last_updated = Instant::now();
                loop {
                    if let Ok(Err(_)) = event_stream
                        .next()
                        .with_timeout(Duration::from_secs(REFRESH_TIMEOUT_SECS))
                        .await
                    {
                        return;
                    }
                    if last_updated.elapsed() >= Duration::from_secs(REFRESH_THROTTLE_SECS) {
                        break;
                    }
                }
            }
        });
        Ok(())
    }
}
