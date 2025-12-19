use crate::datatypes::TrackerStatus;
use proton_core_common::datatypes::UnixTimestamp;

#[derive(Clone, Debug)]
pub struct TrackerInfo {
    pub status: TrackerStatus,
    pub trackers: Vec<TrackerDomain>,
    pub last_checked_at: UnixTimestamp,
}

#[derive(Clone, Debug)]
pub struct TrackerDomain {
    pub name: String,
    pub urls: Vec<String>,
}
