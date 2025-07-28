use crate::core::datatypes::UnixTimestamp;
use crate::{UniffiEnum, UniffiRecord};

#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct SnoozeActions {
    pub options: Vec<SnoozeTime>,
    pub show_unsnooze: bool,
}

#[derive(Debug, Clone, PartialEq, UniffiEnum)]
pub enum SnoozeTime {
    Tomorrow(UnixTimestamp),
    LaterThisWeek(UnixTimestamp),
    ThisWeekend(UnixTimestamp),
    NextWeek(UnixTimestamp),
    Custom(UnixTimestamp),
}
