use jiff::Span;
use proton_core_api::declare_proton_id;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

declare_proton_id! {
    pub CalendarId
}
declare_proton_id! {
    pub CalendarEventId
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

declare_proton_id! {
    /// Event identifier, shared across event repetitions.
    ///
    /// This is different from [`CalendarEventId`] in two important ways:
    ///
    /// - [`CalendarEventId`] is a Proton-specific identifier of the event, so
    ///   an event uploaded from an external calendar has its own Proton-id, but
    ///   its uid is copy-pasted from the original system.
    ///
    /// - Even though `u` in `uid` here stands for `unique`, it's actually *not*
    ///   unique in the sense of being a primary key - notably, when you have a
    ///   recurring event, you need both this and [`CalendarEventRecurrenceId`]
    ///   in order to retrieve an actually-unique [`CalendarEventId`].
    ///
    ///   (intuitively: when you have a repeating event, all exceptions to the
    ///   repeating schedule - known as single edits - are their own Proton
    ///   events: they have the same UID, but different recurrence ids and
    ///   different event ids.)
    pub CalendarEventUid
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

impl<T> From<T> for CalendarColor
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        Self::new(value.into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarNotification {
    #[serde(rename = "Type")]
    pub ty: CalendarNotificationType,
    pub trigger: Span,
}

impl PartialEq for CalendarNotification {
    fn eq(&self, other: &Self) -> bool {
        self.ty == other.ty && self.trigger.fieldwise() == other.trigger.fieldwise()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u32)]
pub enum CalendarNotificationType {
    Email = 0,
    Push = 1,
}

/// Unix timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarEventRecurrenceId(i64);

impl CalendarEventRecurrenceId {
    #[must_use]
    pub fn new(ts: i64) -> Self {
        Self(ts)
    }

    #[must_use]
    pub fn get(&self) -> i64 {
        self.0
    }
}
