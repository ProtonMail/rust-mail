use crate::MailUserContext;
use crate::datatypes::{LocalMessageId, TrackerDomain, TrackerInfo, TrackerStatus};
use crate::models::{TrackedMessage, TrackingUrl};
use anyhow::Context;
use proton_core_api::services::proton::ProtonCore;
use proton_core_common::datatypes::UnixTimestamp;
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::collections::{HashMap, HashSet};
use std::sync::Weak;
use url::Url;

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
    ) -> Result<TrackerStatus, StashError> {
        let ctx = self.ctx.upgrade().context("Could not find the context")?;
        let mut tether = ctx.user_stash().connection().await?;

        if urls.is_empty() {
            tether
                .tx(async |tx| {
                    TrackedMessage {
                        local_message_id: message_id,
                        status: TrackerStatus::NoTrackers,
                        last_checked_at: UnixTimestamp::now(),
                    }
                    .save(tx)
                    .await
                })
                .await?;
            return Ok(TrackerStatus::NoTrackers);
        }

        let mut found_trackers = Vec::new();

        for url in urls {
            match self.check_url(&url).await {
                Ok(Some(tracker_domain)) => {
                    found_trackers.push((tracker_domain, url));
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Failed to check URL {}: {:?}", url, e);
                }
            }
        }

        let status = if found_trackers.is_empty() {
            TrackerStatus::NoTrackers
        } else {
            TrackerStatus::Trackers
        };

        tether
            .tx(async |tx| {
                TrackedMessage {
                    local_message_id: message_id,
                    status,
                    last_checked_at: UnixTimestamp::now(),
                }
                .save(tx)
                .await?;

                TrackingUrl::delete_by_message(message_id, tx).await?;

                for (tracker_domain, original_url) in found_trackers {
                    TrackingUrl {
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

        Ok(status)
    }

    pub async fn get_tracker_info(
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<TrackerInfo, StashError> {
        let tracked = TrackedMessage::load(message_id, tether).await?;

        let (status, last_checked_at) = if let Some(tracked) = tracked {
            (tracked.status, tracked.last_checked_at)
        } else {
            (TrackerStatus::Unknown, UnixTimestamp::default())
        };

        let tracking_urls = TrackingUrl::find_by_message(message_id, tether).await?;

        let mut domains: HashMap<String, Vec<String>> = HashMap::new();

        for url in tracking_urls {
            domains
                .entry(url.tracker_domain)
                .or_default()
                .push(url.original_url);
        }

        let trackers = domains
            .into_iter()
            .map(|(name, urls)| TrackerDomain { name, urls })
            .collect();

        Ok(TrackerInfo {
            status,
            trackers,
            last_checked_at,
        })
    }
}
