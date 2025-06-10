use proton_core_api::declare_proton_id;
use serde::{Deserialize, Serialize};

declare_proton_id! {
    pub CalendarId
}
declare_proton_id! {
    pub CalendarEventId
}
declare_proton_id! {
    pub CalendarEventRecurrenceId
}
declare_proton_id! {
    pub CalendarMemberId
}
declare_proton_id! {
    pub CalendarKeyId
}
declare_proton_id! {
    pub CalendarAttendeeId
}
declare_proton_id! {
    pub CalendarAttendeeToken
}

/// Hex-color, like "#ff0000"; used both for calendars and events themselves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarColor(String);

impl CalendarColor {
    #[must_use]
    pub fn new(color: impl Into<String>) -> Self {
        Self(color.into())
    }

    #[must_use]
    pub fn get(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CalendarColor {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}
