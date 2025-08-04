use crate::{
    CalendarAttendeeId, CalendarAttendeeToken, CalendarColor, CalendarEventId, CalendarId,
    CalendarKeyId, CalendarMemberId, CalendarNotification,
};
use proton_core_api::services::proton::AddressId;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{BoolFromInt, serde_as};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarBootstrap {
    pub keys: Vec<CalendarKey>,
    pub passphrase: CalendarPassphrase,

    // While this is technically an array, backend always returns one member.
    //
    // This is a historical artifact - they thought multi-member calendars will
    // come handy for sharing, but eventually this design was abandoned.
    pub members: [CalendarMember; 1],
}

impl CalendarBootstrap {
    #[must_use]
    pub fn member(&self) -> &CalendarMember {
        &self.members[0]
    }

    #[must_use]
    pub fn primary_key(&self) -> Option<&CalendarKey> {
        self.keys
            .iter()
            .find(|key| key.flags == CalendarKeyFlags::ActiveAndPrimary)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarMember {
    #[serde(rename = "ID")]
    pub id: CalendarMemberId,
    pub name: String,
    pub color: CalendarColor,
    #[serde(rename = "AddressID")]
    pub address_id: AddressId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarKey {
    #[serde(rename = "ID")]
    pub id: CalendarKeyId,
    pub private_key: String,
    pub flags: CalendarKeyFlags,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u32)]
pub enum CalendarKeyFlags {
    Inactive = 0,
    Active = 1,
    ActiveAndPrimary = 3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarPassphrase {
    pub member_passphrases: Vec<CalendarMemberPassphrase>,
}

impl CalendarPassphrase {
    #[must_use]
    pub fn for_member(&self, id: &CalendarMemberId) -> Option<&CalendarMemberPassphrase> {
        self.member_passphrases
            .iter()
            .find(|pass| pass.member_id == *id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarMemberPassphrase {
    #[serde(rename = "MemberID")]
    pub member_id: CalendarMemberId,
    pub passphrase: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetCalendarEvent {
    pub event: CalendarEvent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FoundCalendarEvents {
    pub events: Vec<CalendarEvent>,
}

#[serde_as]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarEvent {
    #[serde(rename = "ID")]
    pub id: CalendarEventId,
    #[serde(rename = "AddressID")]
    pub address_id: Option<AddressId>,
    pub shared_events: Vec<CalendarEventPayload>,
    pub calendar_events: Vec<CalendarEventPayload>,
    #[serde(rename = "CalendarID")]
    pub calendar_id: CalendarId,
    pub address_key_packet: Option<String>,
    pub shared_key_packet: Option<String>,

    // There's always either zero or one attendee events, so technically this
    // could probably be `Option<[CalendarEventPayload; 1]>` paired with
    // `#[serde(default)]`, but using `Vec` is an easier way out
    pub attendees_events: Vec<CalendarEventPayload>,

    pub attendees: Vec<CalendarAttendee>,
    pub notifications: Option<Vec<CalendarNotification>>,
    pub color: Option<CalendarColor>,
    #[serde_as(as = "BoolFromInt")]
    pub is_proton_proton_invite: bool,
}

impl CalendarEvent {
    #[must_use]
    pub fn attendee_status(&self, token: &CalendarAttendeeToken) -> Option<CalendarAttendeeStatus> {
        self.attendees.iter().find_map(|att| {
            if att.token == *token {
                Some(att.status)
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarEventPayload {
    #[serde(rename = "Type")]
    pub ty: CalendarEventPayloadType,
    pub data: String,
    pub signature: Option<String>,
    pub author: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u32)]
pub enum CalendarEventPayloadType {
    ClearText = 0,
    Encrypted = 1,
    Signed = 2,
    EncryptedAndSigned = 3,
}

impl CalendarEventPayloadType {
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        *self == Self::Encrypted || *self == Self::EncryptedAndSigned
    }

    #[must_use]
    pub fn is_signed(&self) -> bool {
        *self == Self::Signed || *self == Self::EncryptedAndSigned
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarAttendee {
    #[serde(rename = "ID")]
    pub id: CalendarAttendeeId,
    pub token: CalendarAttendeeToken,
    pub status: CalendarAttendeeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(u32)]
pub enum CalendarAttendeeStatus {
    Unanswered = 0,
    Maybe = 1,
    No = 2,  // aka "rejected"
    Yes = 3, // aka "accepted"
}

impl CalendarAttendeeStatus {
    #[must_use]
    pub fn is_unanswered(&self) -> bool {
        matches!(self, Self::Unanswered)
    }

    /// Returns whether this status warrants notifying the user (e.g. whether we
    /// should send a push notification).
    #[must_use]
    pub fn should_notify(&self) -> bool {
        matches!(self, Self::Maybe | Self::Yes)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarVTimezones {
    pub timezones: BTreeMap<CalendarVTimezoneName, CalendarVTimezoneIcs>,
}

// e.g. "Europe/London"
pub type CalendarVTimezoneName = String;

// e.g. "BEGIN:VTIMEZONE..."
pub type CalendarVTimezoneIcs = String;

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    const ENCRYPTED: bool = true;
    const NOT_ENCRYPTED: bool = false;

    const SIGNED: bool = true;
    const NOT_SIGNED: bool = false;

    const NOTIFY: bool = true;
    const DONT_NOTIFY: bool = false;

    #[test_case(CalendarEventPayloadType::ClearText, NOT_SIGNED, NOT_ENCRYPTED)]
    #[test_case(CalendarEventPayloadType::Encrypted, NOT_SIGNED, ENCRYPTED)]
    #[test_case(CalendarEventPayloadType::Signed, SIGNED, NOT_ENCRYPTED)]
    #[test_case(CalendarEventPayloadType::EncryptedAndSigned, SIGNED, ENCRYPTED)]
    fn is_signed_or_encrypted(
        target: CalendarEventPayloadType,
        is_signed: bool,
        is_encrypted: bool,
    ) {
        assert_eq!(is_signed, target.is_signed());
        assert_eq!(is_encrypted, target.is_encrypted());
    }

    #[test_case(CalendarAttendeeStatus::Unanswered, DONT_NOTIFY)]
    #[test_case(CalendarAttendeeStatus::Maybe, NOTIFY)]
    #[test_case(CalendarAttendeeStatus::No, DONT_NOTIFY)]
    #[test_case(CalendarAttendeeStatus::Yes, NOTIFY)]
    fn should_notify(target: CalendarAttendeeStatus, expected: bool) {
        assert_eq!(expected, target.should_notify());
    }
}
