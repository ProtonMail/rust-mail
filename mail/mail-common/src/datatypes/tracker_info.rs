use std::collections::BTreeSet;

use proton_core_common::datatypes::UnixTimestamp;

#[derive(Clone, Debug)]
pub struct TrackerInfo {
    pub trackers: BTreeSet<TrackerDomain>,
    pub last_checked_at: UnixTimestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrackerDomain {
    pub name: String,
    pub urls: BTreeSet<String>,
}
