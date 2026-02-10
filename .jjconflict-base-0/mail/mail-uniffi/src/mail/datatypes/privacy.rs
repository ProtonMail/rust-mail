use proton_mail_common::datatypes::{
    PrivacyInfo as RealPrivacyInfo, PrivacyInfoStatus as RealPrivacyInfoStatus,
    StrippedUTMInfo as RealStrippedUTMInfo, TrackerDomain as RealTrackerDomain,
    TrackerInfo as RealTrackerInfo, UTMLink as RealUTMLink,
};

#[derive(Clone, Debug, uniffi::Record)]
pub struct TrackerInfo {
    pub trackers: Vec<TrackerDomain>,
    pub last_checked_at: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TrackerDomain {
    pub name: String,
    pub urls: Vec<String>,
}

impl From<RealTrackerInfo> for TrackerInfo {
    fn from(info: RealTrackerInfo) -> Self {
        Self {
            trackers: info.trackers.into_iter().map(Into::into).collect(),
            last_checked_at: info.last_checked_at.as_u64(),
        }
    }
}

impl From<RealTrackerDomain> for TrackerDomain {
    fn from(domain: RealTrackerDomain) -> Self {
        Self {
            name: domain.name,
            urls: domain.urls.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct StrippedUTMInfo {
    pub links: Vec<UTMLink>,
}
impl From<RealStrippedUTMInfo> for StrippedUTMInfo {
    fn from(info: RealStrippedUTMInfo) -> Self {
        Self {
            links: info.links.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UTMLink {
    pub original_url: String,
    pub cleaned_url: String,
}

impl From<RealUTMLink> for UTMLink {
    fn from(link: RealUTMLink) -> Self {
        Self {
            original_url: link.original_url,
            cleaned_url: link.cleaned_url,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PrivacyInfo {
    pub trackers: TrackerInfoWithStatus,
    pub utm_links: Option<StrippedUTMInfo>,
}
impl From<RealPrivacyInfo> for PrivacyInfo {
    fn from(info: RealPrivacyInfo) -> Self {
        Self {
            trackers: info.trackers.into(),
            utm_links: info.utm_links.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum TrackerInfoWithStatus {
    Pending,
    /// User disabled using Image Proxy
    Disabled,
    Detected(TrackerInfo),
}

impl From<RealPrivacyInfoStatus<RealTrackerInfo>> for TrackerInfoWithStatus {
    fn from(value: RealPrivacyInfoStatus<RealTrackerInfo>) -> Self {
        match value {
            RealPrivacyInfoStatus::Pending => Self::Pending,
            RealPrivacyInfoStatus::Disabled => Self::Disabled,
            RealPrivacyInfoStatus::Detected(o) => Self::Detected(From::from(o)),
        }
    }
}
