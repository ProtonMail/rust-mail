use crate::core::datatypes::UnixTimestamp;
use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::utils::MapVec;
use proton_mail_common::{SnoozeOptions, SnoozeTime as RealSnoozeTime};

#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct SnoozeActions {
    pub options: Vec<SnoozeTime>,
    pub show_unsnooze: bool,
}

impl From<SnoozeOptions> for SnoozeActions {
    fn from(options: SnoozeOptions) -> Self {
        SnoozeActions {
            options: options.options.map_vec(),
            show_unsnooze: options.show_unsnooze,
        }
    }
}

#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum SnoozeTime {
    Tomorrow(UnixTimestamp),
    LaterThisWeek(UnixTimestamp),
    ThisWeekend(UnixTimestamp),
    NextWeek(UnixTimestamp),
    Custom,
}

impl From<RealSnoozeTime> for SnoozeTime {
    fn from(time: RealSnoozeTime) -> Self {
        match time {
            RealSnoozeTime::Tomorrow(timestamp) => SnoozeTime::Tomorrow(timestamp.into()),
            RealSnoozeTime::LaterThisWeek(timestamp) => SnoozeTime::LaterThisWeek(timestamp.into()),
            RealSnoozeTime::ThisWeekend(timestamp) => SnoozeTime::ThisWeekend(timestamp.into()),
            RealSnoozeTime::NextWeek(timestamp) => SnoozeTime::NextWeek(timestamp.into()),
            RealSnoozeTime::Custom => SnoozeTime::Custom,
        }
    }
}
