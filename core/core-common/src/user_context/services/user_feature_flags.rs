//! Note: This service is for per-user feature flags.
//! If you are looking for global feature flags,
//! please see [`crate::services::FeatureFlagsService`].

use std::{collections::BTreeMap, sync::Weak};

use anyhow::Context;
use proton_core_api::{
    service::ApiServiceError,
    services::proton::{
        GetLegacyFeatureFlagsOptions, GetLegacyFeaturesResponse, GetUnleashFeaturesResponse,
        LegacyFeatureFlag, LegacyFeatureFlagType, MAX_LEGACY_FEATURES_PER_PAGE, ProtonCore,
    },
    session::Session,
};
use stash::{
    orm::Model,
    params,
    stash::{Stash, Tether, WatcherHandle},
    watcher::TableWatcher,
};

use crate::{
    CoreContextError, CoreContextResult, UserContext,
    datatypes::{UnixTimestamp, UserFeatureFlagSource},
    models::{ModelExtension, UserFeatureFlag},
    utils::Paginatable,
};

enum FlagPersistence {
    Persist,
    DontPersist,
}

#[derive(Clone)]
pub struct UserFeatureFlagsService {
    ctx: Weak<UserContext>,
}

impl UserFeatureFlagsService {
    #[must_use]
    pub fn new(ctx: Weak<UserContext>) -> Self {
        Self { ctx }
    }

    async fn fetch_from_cache(
        tether: &Tether,
        source: UserFeatureFlagSource,
    ) -> BTreeMap<String, UserFeatureFlag> {
        UserFeatureFlag::find("where source=?", params![source], tether)
            .await
            .inspect_err(|err| tracing::warn!("Failed to fetch feature flags: {}", err))
            .unwrap_or_default()
            .into_iter()
            .map(|flag| (flag.name.clone(), flag))
            .collect::<BTreeMap<String, UserFeatureFlag>>()
    }

    fn set_flags_from_unleash(
        flags: &mut BTreeMap<String, UserFeatureFlag>,
        response: GetUnleashFeaturesResponse,
        modify_time: UnixTimestamp,
    ) {
        tracing::debug!(
            "Fetched {} user featured flags from unleash API",
            response.toggles.len()
        );
        for toggle in response.toggles {
            let flag = flags
                .entry(toggle.name.clone())
                .or_insert_with(|| UserFeatureFlag {
                    name: toggle.name,
                    enabled: false,
                    source: UserFeatureFlagSource::Unleash,
                    writable: false,
                    overridden_to: None,
                    overridden_at: None,
                    modify_time,
                });

            // Currently we are ignoring variants,
            // and Unleash API says that feature is always enabled
            flag.enabled = true;
            flag.source = UserFeatureFlagSource::Unleash;
            flag.writable = false;
            flag.modify_time = modify_time;
        }
    }

    fn set_flags_from_legacy(
        flags: &mut BTreeMap<String, (UserFeatureFlag, FlagPersistence)>,
        api_flags: Vec<LegacyFeatureFlag>,
        now: UnixTimestamp,
    ) {
        let boolean_features = api_flags
            .into_iter()
            .filter(|feature| {
                let Some(expiration_time) =
                    feature.metadata.expiration_time.map(UnixTimestamp::from)
                else {
                    return true;
                };
                expiration_time >= now
            })
            .filter_map(|feature| {
                let LegacyFeatureFlag { metadata, variant } = feature;
                // Currently we support only boolean feature flags.
                let value = variant.into_bool();
                value.map(|value| (metadata, value))
            });

        for (metadata, value) in boolean_features {
            let enabled = value.value;

            let (flag, persistence) = flags.entry(metadata.code.clone()).or_insert_with(|| {
                (
                    UserFeatureFlag {
                        name: metadata.code,
                        enabled,
                        source: UserFeatureFlagSource::Legacy,
                        writable: metadata.writable,
                        overridden_to: None,
                        overridden_at: None,
                        modify_time: now,
                    },
                    FlagPersistence::Persist,
                )
            });

            *persistence = FlagPersistence::Persist;
            if let Some(overridden_at) = flag.overridden_at
                && let Some(remote_update_at) = metadata.update_time
            {
                // Both dates come from the same source - the backend.
                // We never update those fields with device clock.
                // Therefore it is safe to compare those two timestamps.
                let remote_updated_at = UnixTimestamp::from(remote_update_at);
                if overridden_at > remote_updated_at {
                    // This is stale data.
                    tracing::warn!("Stale data for feature flag {}", flag.name);
                    tracing::warn!("Overridden at: {}", overridden_at);
                    tracing::warn!("Remote update at: {}", remote_update_at);
                    tracing::warn!("Flag stays as: {}", flag.enabled);
                    continue;
                }
            }
            flag.enabled = enabled;
            flag.source = UserFeatureFlagSource::Legacy;
            flag.writable = metadata.writable;
            flag.modify_time = now;

            // Overridden at is set only AFTER remote successfully
            if flag.overridden_to.is_some() && flag.overridden_at.is_some() {
                flag.overridden_at = None;
                flag.overridden_to = None;
            }
        }
    }

    async fn refresh_unleash_flags(
        &self,
        api: &Session,
        stash: &Stash,
        modify_time: UnixTimestamp,
    ) -> CoreContextResult<()> {
        let response = api.get_unleash_feature_flags().await?;
        let mut tether = stash.connection().await?;
        let mut flags = Self::fetch_from_cache(&tether, UserFeatureFlagSource::Unleash).await;
        for flag in flags.values_mut() {
            // Unleash returns only enabled flags. We don't want to remove them from cache or keep stale data.
            // But instead, we are marking them with false.
            // The easiest way to do so is to mark everything as disabled and then in `set_flags_from_unleash` mark
            // every present flag as enabled.
            flag.enabled = false;
        }

        Self::set_flags_from_unleash(&mut flags, response, modify_time);

        let flags = flags.into_values().collect();

        tether
            .tx(async |tx| UserFeatureFlag::save_all(flags, tx).await)
            .await?;
        Ok(())
    }

    async fn refresh_legacy_flags(
        &self,
        api: &Session,
        stash: &Stash,
        modify_time: UnixTimestamp,
    ) -> CoreContextResult<()> {
        let initial_flags = GetLegacyFeatureFlagsOptions {
            feature_type: Some(LegacyFeatureFlagType::Boolean),
            ..Default::default()
        };
        let response = PaginateLegacyFeatureFlags::fetch_all_filtered(api, initial_flags).await?;

        let mut tether = stash.connection().await?;
        let mut cached_flags = Self::fetch_from_cache(&tether, UserFeatureFlagSource::Legacy)
            .await
            .into_iter()
            .map(|(key, flag)| (key, (flag, FlagPersistence::DontPersist)))
            .collect::<BTreeMap<_, _>>();

        Self::set_flags_from_legacy(&mut cached_flags, response, modify_time);

        let mut flags_to_remove = Vec::new();
        let mut flags_to_save = Vec::new();

        for (name, (flag, persistence)) in cached_flags {
            match persistence {
                FlagPersistence::Persist => flags_to_save.push(flag),
                FlagPersistence::DontPersist => flags_to_remove.push(name),
            }
        }

        tether
            .tx(async |tx| {
                UserFeatureFlag::delete_batch_from_source(
                    flags_to_remove,
                    UserFeatureFlagSource::Legacy,
                    tx,
                )
                .await?;
                UserFeatureFlag::save_all(flags_to_save, tx).await
            })
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all, name = "UserFeatureFlagsRefresh")]
    pub async fn refresh(&self) -> CoreContextResult<()> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let api = ctx.session();

        let modify_time = UnixTimestamp::now();

        let legacy_flags = self.refresh_legacy_flags(api, ctx.stash(), modify_time);
        let unleash_flags = self.refresh_unleash_flags(api, ctx.stash(), modify_time);

        // We do not use `try_join` here because even if only one endpoint is working, we still want to
        // update those flags.
        let (legacy_flags, unleash_flags) = tokio::join!(legacy_flags, unleash_flags);

        if let Err(error) = legacy_flags {
            tracing::error!(%error, "Failed to refresh legacy flags");
        }
        if let Err(error) = unleash_flags {
            tracing::error!(%error, "Failed to refresh Unleash flags", );
        }

        Ok(())
    }

    pub async fn get(&self, key: &str) -> CoreContextResult<Option<bool>> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let feature_flag = {
            let tether = ctx.stash().connection().await?;
            UserFeatureFlag::by_name(key, &tether).await?
        };
        Ok(feature_flag.map(|flag| flag.is_enabled()))
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

struct PaginateLegacyFeatureFlags;

impl Paginatable for PaginateLegacyFeatureFlags {
    type PaginateOptions = GetLegacyFeatureFlagsOptions;

    type Response = GetLegacyFeaturesResponse;

    type Output = LegacyFeatureFlag;

    type Error = ApiServiceError;

    type API = Session;

    const NAME: &'static str = "Legacy Feature Flags";

    const DEFAULT_PAGE_SIZE: u64 = MAX_LEGACY_FEATURES_PER_PAGE;

    async fn fetch(
        api: &Self::API,
        options: Self::PaginateOptions,
    ) -> Result<Self::Response, Self::Error> {
        api.get_legacy_feature_flags(options).await
    }
}
