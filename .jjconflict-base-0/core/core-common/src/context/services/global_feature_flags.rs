use crate::app_events::{OnEnterForegroundEvent, OnUserContextMapChanged};
use crate::datatypes::UnixTimestamp;
use crate::models::{FeatureFlag, ModelExtension};
use crate::{Context, services::Service};
use crate::{CoreContextError, CoreContextResult, Origin};
use anyhow::{Context as _, Result};
use proton_core_api::services::proton::ProtonCore as _;
use proton_core_api::session::Session;
use stash::stash::WatcherHandle;
use stash::watcher::TableWatcher;
use std::collections::BTreeMap;
use std::sync::Weak;
use std::time::{Duration, Instant};
use tracing::{Instrument, debug, error, info, warn};

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
        info!("Fetched {} featured flags from API", response.toggles.len());

        let mut tether = ctx.account_stash().connection().await?;

        let mut flags = FeatureFlag::all(&tether)
            .await
            .inspect_err(|err| warn!("Failed to fetch feature flags: {}", err))
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

    #[cfg(feature = "test-utils")]
    pub async fn test_override(&self, key: &str, value: bool) -> CoreContextResult<()> {
        use stash::orm::Model;

        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let mut tether = ctx.account_stash().connection().await?;
        let mut flag = FeatureFlag::by_name(key, &tether)
            .await?
            .unwrap_or(FeatureFlag {
                name: key.to_owned(),
                enabled: false,
                modify_time: UnixTimestamp::now(),
            });
        flag.enabled = value;
        tether.tx(async |tx| flag.save(tx).await).await?;

        Ok(())
    }

    pub async fn refresh(&self) -> CoreContextResult<()> {
        let ctx = self.ctx.upgrade().context("Could not upgrade context")?;
        let session = ctx.new_api_session(None).await?;
        self.fetch_and_update(&session).await
    }

    pub async fn list_all(&self) -> Vec<(String, bool)> {
        let Some(ctx) = self.ctx.upgrade() else {
            warn!("Failed to upgrade context");
            return vec![];
        };
        let Ok(tether) = ctx.account_stash().connection().await else {
            warn!("Failed to connect to account stash");
            return vec![];
        };
        let flags = FeatureFlag::all(&tether)
            .await
            .inspect_err(|err| warn!("Failed to fetch feature flags: {}", err))
            .unwrap_or_default();

        info!("Retrieved {} feature flags", flags.len());

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

    #[tracing::instrument(skip(self), name = "FeatureFlagsInit")]
    async fn init(&self) -> Result<(), Self::Error> {
        if self.background_task_setting == FeatureFlagsBackgroundTask::Disabled {
            warn!("Feature flags background task is disabled");
            return Ok(());
        }
        let ctx = self
            .ctx
            .upgrade()
            .context("Could not upgrade context")
            .map_err(CoreContextError::Other)?;

        if ctx.origin() != Origin::App {
            warn!("Feature flags background task is not allowed for non-app origins");
            return Ok(());
        }

        let task_service = ctx.task_service();
        let event_service = ctx.event_service();

        let self_clone = self.clone();
        let Some(mut foreground_event_stream) = event_service.subscribe::<OnEnterForegroundEvent>()
        else {
            error!("Failed to subscribe to OnEnterForegroundEvent");
            return Ok(());
        };
        let Some(mut ctx_map_changed_stream) = event_service.subscribe::<OnUserContextMapChanged>()
        else {
            error!("Failed to subscribe to OnUserContextMapChangedEvent");
            return Ok(());
        };
        task_service.spawn(
            async move {
                let Some(ctx) = self_clone.ctx.upgrade() else {
                    debug!("Could not upgrade context");
                    return;
                };
                let Ok(session) = ctx.new_api_session(None).await else {
                    error!("Failed to create API session");
                    return;
                };
                drop(ctx);
                loop {
                    let Some(ctx) = self_clone.ctx.upgrade() else {
                        debug!("Could not upgrade context");
                        return;
                    };
                    let user_contexts: Vec<_> = ctx
                        .active_user_contexts
                        .lock()
                        .await
                        .values()
                        .cloned()
                        // We clone and collect to release the lock ASAP.
                        .collect::<Vec<_>>()
                        .into_iter()
                        .filter_map(|ctx| if let Some(ctx) = ctx.upgrade() { Some(ctx) } else {
                            debug!("Could not upgrade user context");
                            None
                        })
                        .collect();

                    drop(ctx);

                    if user_contexts.is_empty() {
                        debug!("Refreshing global feature flags");
                        if let Err(error) = self_clone.fetch_and_update(&session).await {
                            error!(%error, "Failed to refresh global feature flags");
                        }
                    } else {
                        for user_ctx in user_contexts {
                            debug!("Refreshing user feature flags for {:?}", user_ctx.user_id());
                            if let Err(error) = user_ctx.feature_flags().refresh().await {
                                error!(%error, "Failed to refresh user feature flags");
                            }
                        }
                    }
                    let last_updated = Instant::now();
                    let timeout = tokio::time::sleep(Duration::from_secs(REFRESH_TIMEOUT_SECS));
                    tokio::pin!(timeout);
                    loop {
                        debug!("Going to sleep");
                        tokio::select! { biased;
                            res = ctx_map_changed_stream.next() => match res {
                                Ok(_) => {
                                    debug!("User context map changed. Waking up");
                                    debug!(
                                        "Woke up after {}/{REFRESH_TIMEOUT_SECS} seconds",
                                        last_updated.elapsed().as_secs()
                                    );
                                    break;
                                },
                                Err(error) => {
                                    error!(?error, "Failed to receive event");
                                    return;
                                }
                            },
                            () = &mut timeout => {
                                debug!("Timeout of {REFRESH_TIMEOUT_SECS} seconds reached. Waking up");
                                break;
                            }
                            res = foreground_event_stream.next() => match res {
                                Ok(_) => {
                                    debug!("onForeground event received. Waking up");

                                    debug!(
                                        "Woke up after {}/{REFRESH_TIMEOUT_SECS} seconds",
                                        last_updated.elapsed().as_secs()
                                    );

                                    if last_updated.elapsed() >= Duration::from_secs(REFRESH_THROTTLE_SECS) {
                                        break;
                                    }
                                    debug!("Woken up before {REFRESH_THROTTLE_SECS} seconds. Ignoring...");
                                }
                                Err(error) => {
                                    error!(?error, "Failed to receive event");
                                    return;
                                }
                            },
                        }
                    }
                }
            }
            .in_current_span(),
        );
        Ok(())
    }
}
