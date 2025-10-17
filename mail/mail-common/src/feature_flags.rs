use std::collections::BTreeMap;
use std::sync::Weak;
use std::time::Duration;

use anyhow::{Context as _, Result};
use proton_core_api::session::Session;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::{FeatureFlag, ModelExtension};
use proton_core_common::{Context, services::Service};
use proton_core_common::{CoreContextError, CoreContextResult};
use proton_mail_api::services::proton::ProtonMail;

use stash::stash::WatcherHandle;
use stash::watcher::TableWatcher;
use tracing::error;

#[derive(Clone)]
pub struct FeatureFlagsService {
    ctx: Weak<Context>,
}

impl FeatureFlagsService {
    pub fn new(ctx: Weak<Context>) -> Self {
        Self { ctx }
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
                    id: None,
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

const REFRESH_INTERVAL_SECS: u64 = 600; // 10 minutes

#[async_trait::async_trait]
impl Service for FeatureFlagsService {
    type Error = CoreContextError;

    async fn init(&self) -> Result<(), Self::Error> {
        let ctx = self
            .ctx
            .upgrade()
            .expect("Context to be there during initialization");

        let task_service = ctx.task_service();
        let self_clone = self.clone();
        task_service.spawn(async move {
            loop {
                if let Err(error) = self_clone.refresh().await {
                    error!(%error, "Failed to refresh feature flags");
                };
                tokio::time::sleep(Duration::from_secs(REFRESH_INTERVAL_SECS)).await;
            }
        });
        Ok(())
    }
}
