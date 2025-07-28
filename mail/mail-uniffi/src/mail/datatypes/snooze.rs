use crate::core::datatypes::UnixTimestamp;
use crate::{UniffiEnum, UniffiRecord};

#[derive(Debug, Clone, PartialEq, UniffiRecord)]
pub struct SnoozeActions {
    pub options: Vec<SnoozeTime>,
    pub show_unsnooze: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, UniffiEnum)]
pub enum SnoozeTime {
    Tomorrow(Weekday),
    LaterThisWeek(Weekday),
    ThisWeekend(Weekday),
    NextWeek(Weekday),
    Custom(UnixTimestamp),
}

#[derive(Debug, Clone, Copy, PartialEq, UniffiEnum)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}
