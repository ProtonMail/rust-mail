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

        let new_map: BTreeMap<String, bool> = response
            .toggles
            .into_iter()
            // Currently we are ignoring variants, and Unleash API says that feature is always enabled
            .map(|toggle| (toggle.name, true))
            .collect();

        tracing::info!("Fetched {} featured flags from API", new_map.len());

        let modify_time = UnixTimestamp::now();

        let flags = new_map
            .into_iter()
            .map(|(name, enabled)| FeatureFlag {
                id: None,
                name,
                enabled,
                modify_time,
            })
            .collect();

        let mut tether = ctx.account_stash().connection().await?;

        tether
            .tx(async |tx| FeatureFlag::replace(flags, tx).await)
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
            return vec![];
        };
        let Ok(tether) = ctx.account_stash().connection().await else {
            return vec![];
        };
        let flags = FeatureFlag::all(&tether).await.unwrap_or_default();

        flags
            .iter()
            .map(|flag| (flag.name.clone(), flag.enabled))
            .collect()
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
            if let Err(error) = self_clone.refresh().await {
                error!(%error, "Failed to refresh feature flags");
            };

            loop {
                tokio::time::sleep(Duration::from_secs(REFRESH_INTERVAL_SECS)).await;
                if let Err(error) = self_clone.refresh().await {
                    error!(%error, "Failed to refresh feature flags");
                };
            }
        });
        Ok(())
    }
}
