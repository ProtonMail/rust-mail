use super::{StrippedUTMInfo, TrackerInfo};

#[derive(Clone, Debug)]
pub struct PrivacyInfo {
    pub trackers: Option<TrackerInfo>,
    pub utm_links: Option<StrippedUTMInfo>,
}
