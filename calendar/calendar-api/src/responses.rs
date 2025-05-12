use super::{
    CalendarAttendeeId, CalendarAttendeeToken, CalendarId, CalendarKeyId, CalendarMemberId,
};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::{BoolFromInt, serde_as};

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
    pub fn into_member(self) -> CalendarMember {
        let [member] = self.members;

        member
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
    pub color: String,
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
pub struct FoundCalendarEvents {
    pub events: Vec<CalendarEvent>,
}

#[serde_as]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarEvent {
    pub shared_events: Vec<CalendarEventPayload>,
    #[serde(rename = "CalendarID")]
    pub calendar_id: CalendarId,
    pub start_time: i64,
    pub end_time: i64,
    #[serde(rename = "FullDay")]
    #[serde_as(as = "BoolFromInt")]
    pub full_day: bool,
    #[serde(rename = "RecurrenceID")]
    pub recurrence_id: Option<String>,
    pub address_key_packet: Option<String>,
    pub shared_key_packet: Option<String>,
    pub attendees_events: Vec<CalendarEventPayload>,
    pub attendees: Vec<CalendarAttendee>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarEventPayload {
    #[serde(rename = "Type")]
    pub ty: CalendarEventPayloadType,
    pub data: String,
    //
    // Each event can also be - and usually is - signed:
    //
    //     pub signature: Option<String>,
    //
    // ... but we don't check those signatures at the moment.
    //
    // Validating the calendar key's signature - which we *do* - is sufficient
    // to prove that nobody has messed with the calendar, while validating each
    // event is a bit PITA (you have to fetch event owner's public keys etc.).
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

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    const ENCRYPTED: bool = true;
    const NOT_ENCRYPTED: bool = false;

    const SIGNED: bool = true;
    const NOT_SIGNED: bool = false;

    #[test_case(CalendarEventPayloadType::ClearText, NOT_SIGNED, NOT_ENCRYPTED)]
    #[test_case(CalendarEventPayloadType::Encrypted, NOT_SIGNED, ENCRYPTED)]
    #[test_case(CalendarEventPayloadType::Signed, SIGNED, NOT_ENCRYPTED)]
    #[test_case(CalendarEventPayloadType::EncryptedAndSigned, SIGNED, ENCRYPTED)]
    fn payload_types(target: CalendarEventPayloadType, is_signed: bool, is_encrypted: bool) {
        assert_eq!(is_signed, target.is_signed());
        assert_eq!(is_encrypted, target.is_encrypted());
    }
}
