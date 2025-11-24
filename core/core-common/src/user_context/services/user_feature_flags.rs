//! Note: This service is for per-user feature flags.
//! If you are looking for global feature flags,
//! please see [`crate::services::FeatureFlagsService`].

use std::{
    collections::BTreeMap,
    sync::Weak,
    time::{Duration, Instant},
};

use anyhow::Context;
use proton_core_api::{
    services::proton::{ProtonCore, muon::common::WithTimeout},
    session::Session,
};
use stash::{stash::WatcherHandle, watcher::TableWatcher};
use tracing::error;

use crate::{
    CoreContextError, CoreContextResult, UserContext,
    app_events::OnEnterForegroundEvent,
    datatypes::UnixTimestamp,
    models::{ModelExtension, UserFeatureFlag},
};

// Note: There are identical constants in global service:
const REFRESH_THROTTLE_SECS: u64 = 60; // 1 minute
const REFRESH_TIMEOUT_SECS: u64 = 600; // 10 minutes

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFeatureFlagsBackgroundTask {
    Enabled,
    Disabled,
}

#[derive(Clone)]
pub struct UserFeatureFlagsService {
    ctx: Weak<UserContext>,
    background_task_setting: UserFeatureFlagsBackgroundTask,
}

impl UserFeatureFlagsService {
    #[must_use]
    pub fn new(
        ctx: Weak<UserContext>,
        background_task_setting: UserFeatureFlagsBackgroundTask,
    ) -> Self {
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

        let mut tether = ctx.stash().connection().await?;

        let mut flags = UserFeatureFlag::all(&tether)
            .await
            .inspect_err(|err| tracing::warn!("Failed to fetch feature flags: {}", err))
            .unwrap_or_default()
            .into_iter()
            .map(|flag| {
                (
                    flag.name.clone(),
                    UserFeatureFlag {
                        // If the flag is not fetched from API but exists in the database,
                        // we mark it as disabled.
                        enabled: false,
                        ..flag
                    },
                )
            })
            .collect::<BTreeMap<String, UserFeatureFlag>>();

        let modify_time = UnixTimestamp::now();

        for toggle in response.toggles {
            let flag = flags
                .entry(toggle.name.clone())
                .or_insert_with(|| UserFeatureFlag {
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
            .tx(async |tx| UserFeatureFlag::save_all(flags, tx).await)
            .await?;

        Ok(())
    }

    pub async fn get(&self, key: &str) -> CoreContextResult<Option<bool>> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let feature_flag = {
            let tether = ctx.stash().connection().await?;
            UserFeatureFlag::by_name(key, &tether).await?
        };
        Ok(feature_flag.map(|flag| flag.enabled))
    }

    pub async fn refresh(&self) -> CoreContextResult<()> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        self.fetch_and_update(ctx.session()).await
    }

    pub async fn list_all(&self) -> Vec<(String, bool)> {
        let Some(ctx) = self.ctx.upgrade() else {
            tracing::warn!("Failed to upgrade context");
            return vec![];
        };
        let Ok(tether) = ctx.stash().connection().await else {
            tracing::warn!("Failed to connect to account stash");
            return vec![];
        };
        let flags = UserFeatureFlag::all(&tether)
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

        let stash = ctx.stash();
        TableWatcher::<UserFeatureFlag>::watch(stash)
            .await
            .map_err(CoreContextError::from)
    }

    #[allow(clippy::result_large_err)]
    pub fn init(&self) -> CoreContextResult<()> {
        if self.background_task_setting == UserFeatureFlagsBackgroundTask::Disabled {
            tracing::warn!("User feature flags background task is disabled");
            return Ok(());
        }

        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;

        let task_service = ctx.context.task_service();
        let event_service = ctx.context.event_service();

        let self_clone = self.clone();
        let Some(mut event_stream) = event_service.subscribe::<OnEnterForegroundEvent>() else {
            error!("Failed to subscribe to OnEnterForegroundEvent");
            return Ok(());
        };

        // This task will be cancelled when user context is removed.
        // Which happens when user logs-out
        task_service.spawn(async move {
            loop {
                let Some(ctx) = self_clone.ctx.upgrade() else {
                    error!("Failed to upgrade context");
                    return;
                };
                let session = ctx.session();
                if let Err(error) = self_clone.fetch_and_update(session).await {
                    error!(%error, "Failed to refresh user feature flags");
                }
                let last_updated = Instant::now();
                loop {
                    if let Err(error) = event_stream
                        .next()
                        .with_timeout(Duration::from_secs(REFRESH_TIMEOUT_SECS))
                        .await
                    {
                        tracing::error!(%error, "Failed to receive OnEnterForegroundEvent");
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
