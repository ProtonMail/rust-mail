//! Note: This service is for per-user feature flags.
//! If you are looking for global feature flags,
//! please see [`crate::services::FeatureFlagsService`].

use std::{collections::BTreeMap, sync::Weak};

use anyhow::Context;
use proton_core_api::services::proton::ProtonCore;
use stash::{stash::WatcherHandle, watcher::TableWatcher};

use crate::{
    CoreContextError, CoreContextResult, UserContext,
    datatypes::UnixTimestamp,
    models::{ModelExtension, UserFeatureFlag},
};

#[derive(Clone)]
pub struct UserFeatureFlagsService {
    ctx: Weak<UserContext>,
}

impl UserFeatureFlagsService {
    #[must_use]
    pub fn new(ctx: Weak<UserContext>) -> Self {
        Self { ctx }
    }

    #[tracing::instrument(skip_all, name = "UserFeatureFlagsRefresh")]
    pub async fn refresh(&self) -> CoreContextResult<()> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let api = ctx.session();

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
}
