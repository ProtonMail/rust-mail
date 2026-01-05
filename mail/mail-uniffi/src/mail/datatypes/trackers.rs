use proton_mail_common::datatypes::{
    TrackerDomain as RealTrackerDomain, TrackerInfo as RealTrackerInfo,
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
