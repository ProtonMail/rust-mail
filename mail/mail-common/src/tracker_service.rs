use crate::MailUserContext;
use crate::datatypes::{
    LocalMessageId, PrivacyInfo, PrivacyInfoStatus, StrippedUTMInfo, TrackerDomain, TrackerInfo,
    UTMLink,
};
use crate::models::{
    MailSettings, MessageTracker, MessageTrackerUrl, MessageUtmLink, MessageUtmLinkUrl,
};
use anyhow::Context;
use proton_core_api::services::proton::ProtonCore;
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::ModelExtension;
use proton_mail_html_transformer::utm::StrippedUTM;
use sqlite_watcher::watcher::TableObserver;
use stash::orm::Model;
use stash::stash::{StashError, Tether, WatcherHandle};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Weak;
use std::time::Duration;
use url::Url;

const CHECK_INTERVAL: Duration =
    Duration::from_secs(60 /*s*/ * 60 /*m*/ * 24 /*h*/ * 3 /*d */);

pub struct PrivacyWatchData {
    pub initial: PrivacyInfo,
    pub handle: WatcherHandle,
}

pub struct TrackerService {
    ctx: Weak<MailUserContext>,
}

impl TrackerService {
    pub fn new(ctx: Weak<MailUserContext>) -> Self {
        Self { ctx }
    }

    async fn check_url(&self, url: &str) -> anyhow::Result<Option<String>> {
        let ctx = self.ctx.upgrade().context("Could not find the context")?;
        let url = Url::parse(url)?;
        let response = ctx.session().proxy_img(&url, true).await?;
        Ok(response.tracker_provider)
    }

    pub async fn update(
        &self,
        message_id: LocalMessageId,
        urls: HashSet<String>,
        utm_stripped: BTreeSet<StrippedUTM>,
    ) -> Result<(), StashError> {
        let ctx = self.ctx.upgrade().context("Could not find the context")?;
        let mut tether = ctx.user_stash().connection().await?;

        let mut found_trackers = Vec::new();
        let now = UnixTimestamp::now();

        tracing::info!("Stripped {} UTM links", utm_stripped.len());

        if MessageUtmLink::load(message_id, &tether).await?.is_none() {
            tracing::info!("Storing UTM info ({}) in cache", utm_stripped.len());
            tether
                .tx(async |tx| {
                    MessageUtmLink {
                        local_message_id: message_id,
                    }
                    .save(tx)
                    .await?;

                    for utm in utm_stripped {
                        MessageUtmLinkUrl {
                            id: None,
                            local_message_id: message_id,
                            original_url: utm.original.to_string(),
                            cleaned_url: utm.cleaned.to_string(),
                        }
                        .save(tx)
                        .await?;
                    }

                    Ok::<_, StashError>(())
                })
                .await?;
        }

        let use_proxy = MailSettings::get_or_default(&tether)
            .await
            .is_proxy_enabled();

        if !use_proxy {
            tracing::info!("User has Image Proxy disabled. Skipping tracker checking");
            return Ok(());
        }

        if let Some(tracker) = MessageTracker::find_by_id(message_id, &tether).await? {
            let last_checked_at = tracker.last_checked_at.to_date_time_utc();
            let now_utc = now.to_date_time_utc();

            if let Some(last_checked_at) = last_checked_at
                && let Some(now_utc) = now_utc
                && let Ok(duration) = (now_utc - last_checked_at).to_std()
                && duration <= CHECK_INTERVAL
            {
                tracing::info!("Message was checked recently. Skipping tracker queries.");
                return Ok(());
            }
        }
        tracing::info!("Found urls: {}", urls.len());

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

        tracing::info!("Found {} trackers", found_trackers.len());

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

    pub async fn get_info(&self, message_id: LocalMessageId) -> Result<PrivacyInfo, StashError> {
        let ctx = self.ctx.upgrade().context("Could not find the context")?;
        let tether = ctx.user_context().stash().connection().await?;

        Self::get_info_inner(&tether, message_id).await
    }

    async fn get_info_inner(
        tether: &Tether,
        message_id: LocalMessageId,
    ) -> Result<PrivacyInfo, StashError> {
        let trackers = Self::get_tracker_info(tether, message_id).await?;
        let utm_links = Self::get_utm_info(tether, message_id).await?;

        Ok(PrivacyInfo {
            trackers,
            utm_links,
        })
    }

    async fn get_tracker_info(
        tether: &Tether,
        message_id: LocalMessageId,
    ) -> Result<PrivacyInfoStatus<TrackerInfo>, StashError> {
        let use_proxy = MailSettings::get_or_default(tether)
            .await
            .is_proxy_enabled();

        if !use_proxy {
            return Ok(PrivacyInfoStatus::Disabled);
        }

        let Some(tracked) = MessageTracker::load(message_id, tether).await? else {
            return Ok(PrivacyInfoStatus::Pending);
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

        Ok(PrivacyInfoStatus::Detected(TrackerInfo {
            trackers,
            last_checked_at,
        }))
    }

    pub async fn get_utm_info(
        tether: &Tether,
        message_id: LocalMessageId,
    ) -> Result<Option<StrippedUTMInfo>, StashError> {
        if MessageUtmLink::load(message_id, tether).await?.is_none() {
            return Ok(None);
        }

        let utm_urls = MessageUtmLinkUrl::find_by_message(message_id, tether).await?;

        let links = utm_urls
            .into_iter()
            .map(|url| UTMLink {
                original_url: url.original_url,
                cleaned_url: url.cleaned_url,
            })
            .collect();

        Ok(Some(StrippedUTMInfo { links }))
    }

    pub async fn watch(&self, message_id: LocalMessageId) -> Result<PrivacyWatchData, StashError> {
        let ctx = self.ctx.upgrade().context("Could not find the context")?;
        let tether = ctx.user_context().stash().connection().await?;
        let initial = Self::get_info_inner(&tether, message_id).await?;

        let handle = tether.subscribe_to(move |sender| Box::new(PrivacyDataWatcher { sender }))?;

        Ok(PrivacyWatchData { initial, handle })
    }
}

pub struct PrivacyDataWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for PrivacyDataWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            MessageTracker::table_name().to_string(),
            MessageTrackerUrl::table_name().to_string(),
            MessageUtmLink::table_name().to_string(),
            MessageUtmLinkUrl::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _tables: &BTreeSet<String>) {
        if let Err(e) = self.sender.send(()) {
            tracing::error!("Failed to send notification for tracker changes: {}", e);
        }
    }
}
