use crate::MailUserContext;
use crate::datatypes::{LocalMessageId, TrackerDomain, TrackerInfo};
use crate::models::{MessageTracker, MessageTrackerUrl};
use anyhow::Context;
use proton_core_api::services::proton::ProtonCore;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::ModelExtension;
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Weak;
use std::time::Duration;
use url::Url;

const CHECK_INTERVAL: Duration =
    Duration::from_secs(60 /*s*/ * 60 /*m*/ * 24 /*h*/ * 3 /*d */);

pub struct TrackerDetector {
    ctx: Weak<MailUserContext>,
}

impl TrackerDetector {
    pub fn new(ctx: Weak<MailUserContext>) -> Self {
        Self { ctx }
    }

    pub async fn check_url(&self, url: &str) -> anyhow::Result<Option<String>> {
        let ctx = self.ctx.upgrade().context("Could not find the context")?;
        let url = Url::parse(url)?;
        let response = ctx.session().proxy_img(&url, true).await?;
        Ok(response.tracker_provider)
    }

    pub async fn check_message_trackers(
        &self,
        message_id: LocalMessageId,
        urls: HashSet<String>,
    ) -> Result<(), StashError> {
        let ctx = self.ctx.upgrade().context("Could not find the context")?;
        let mut tether = ctx.user_stash().connection().await?;

        let mut found_trackers = Vec::new();
        let now = UnixTimestamp::now();

        if let Some(tracker) = MessageTracker::find_by_id(message_id, &tether).await? {
            let last_checked_at = tracker.last_checked_at.to_date_time_utc();
            let now_utc = now.to_date_time_utc();

            if let Some(last_checked_at) = last_checked_at
                && let Some(now_utc) = now_utc
                && let Ok(duration) = (now_utc - last_checked_at).to_std()
                && duration <= CHECK_INTERVAL
            {
                return Ok(());
            }
        }

        for url in urls {
            match self.check_url(&url).await {
                Ok(Some(tracker_domain)) => {
                    found_trackers.push((tracker_domain, url));
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Failed to check URL: {:?}", e);
                }
            }
        }

        tether
            .tx(async |tx| {
                MessageTracker {
                    local_message_id: message_id,
                    last_checked_at: now,
                }
                .save(tx)
                .await?;

                MessageTrackerUrl::delete_by_message(message_id, tx).await?;

                for (tracker_domain, original_url) in found_trackers {
                    MessageTrackerUrl {
                        id: None,
                        local_message_id: message_id,
                        tracker_domain,
                        original_url,
                    }
                    .save(tx)
                    .await?;
                }

                Ok::<_, StashError>(())
            })
            .await?;

        Ok(())
    }

    pub async fn get_tracker_info(
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<TrackerInfo>, StashError> {
        let Some(tracked) = MessageTracker::load(message_id, tether).await? else {
            return Ok(None);
        };

        let last_checked_at = tracked.last_checked_at;

        let tracking_urls = MessageTrackerUrl::find_by_message(message_id, tether).await?;

        let mut domains: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for url in tracking_urls {
            domains
                .entry(url.tracker_domain)
                .or_default()
                .insert(url.original_url);
        }

        let trackers = domains
            .into_iter()
            .map(|(name, urls)| TrackerDomain { name, urls })
            .collect();

        Ok(Some(TrackerInfo {
            trackers,
            last_checked_at,
        }))
    }
}
