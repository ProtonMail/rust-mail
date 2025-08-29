use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use anyhow::{Context as _, Result};
use proton_core_api::session::Session;
use proton_core_common::{Context, models::AppSettings, services::Service};
use proton_core_common::{CoreContextError, CoreContextResult};
use proton_mail_api::services::proton::ProtonMail;

use stash::orm::Model;
use tokio::sync::RwLock;
use tracing::{debug, error};

#[derive(Clone)]
pub struct FeatureFlagsService {
    flags: Arc<RwLock<BTreeMap<String, bool>>>,
    ctx: Weak<Context>,
}

impl FeatureFlagsService {
    pub fn new(ctx: Weak<Context>) -> Self {
        Self {
            flags: Arc::new(RwLock::new(BTreeMap::new())),
            ctx,
        }
    }

    async fn load_from_cache(&self, ctx: &Context) {
        let tether = ctx.account_stash().connection();
        let app_settings = AppSettings::get_or_default(&tether).await;

        {
            let mut guard = self.flags.write().await;
            *guard = app_settings.app_features.features;
        }

        let count = self.flags.read().await.len();
        debug!(%count, "Loaded feature flags from cache");
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

        let mut tether = ctx.account_stash().connection();

        tether
            .tx(async |tx| {
                let mut app_settings = AppSettings::get_or_default(tx).await;
                app_settings.app_features.features = new_map.clone();
                app_settings.save(tx).await
            })
            .await?;

        let mut guard = self.flags.write().await;
        *guard = new_map;

        Ok(())
    }

    pub async fn get(&self, key: &str) -> Option<bool> {
        self.flags.read().await.get(key).copied()
    }

    pub async fn refresh(&self) -> CoreContextResult<()> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let session = ctx.new_api_session(None).await?;
        self.fetch_and_update(&session).await
    }

    pub async fn list_all(&self) -> Vec<(String, bool)> {
        let flags = self.flags.read().await;
        flags.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }
}

#[async_trait::async_trait]
impl Service for FeatureFlagsService {
    type Error = CoreContextError;

    async fn init(&self) -> Result<(), Self::Error> {
        let ctx = self
            .ctx
            .upgrade()
            .expect("Context to be there during initialization");

        self.load_from_cache(&ctx).await;

        let task_service = ctx.task_service();
        let self_clone = self.clone();
        task_service.spawn(async move {
            if let Err(error) = self_clone.refresh().await {
                error!(%error, "Failed to refresh feature flags");
            };
        });
        Ok(())
    }
}
