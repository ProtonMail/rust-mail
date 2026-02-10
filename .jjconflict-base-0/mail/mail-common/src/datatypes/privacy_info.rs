use super::{StrippedUTMInfo, TrackerInfo};

#[derive(Clone, Debug)]
pub struct PrivacyInfo {
    pub trackers: PrivacyInfoStatus<TrackerInfo>,
    // We use Option for utm links because there is no way to disable that cleanup.
    pub utm_links: Option<StrippedUTMInfo>,
}

#[derive(Clone, Debug)]
pub enum PrivacyInfoStatus<T> {
    Pending,
    Disabled,
    Detected(T),
}

impl<T> PrivacyInfoStatus<T> {
    pub fn is_pending(&self) -> bool {
        matches!(self, PrivacyInfoStatus::Pending)
    }

    pub fn is_disabled(&self) -> bool {
        matches!(self, PrivacyInfoStatus::Disabled)
    }

    pub fn is_detected(&self) -> bool {
        matches!(self, PrivacyInfoStatus::Detected(_))
    }

    pub fn as_detected(&self) -> Option<&T> {
        match self {
            PrivacyInfoStatus::Detected(t) => Some(t),
            _ => None,
        }
    }

    pub fn into_detected(self) -> Option<T> {
        match self {
            PrivacyInfoStatus::Detected(t) => Some(t),
            _ => None,
        }
    }
}
