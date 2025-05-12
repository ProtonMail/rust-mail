mod common;
mod responses;

#[cfg(feature = "test-utils")]
mod test_utils;

pub use self::common::*;
pub use self::responses::*;

#[cfg(feature = "test-utils")]
pub use self::test_utils::*;

use muon::{GET, http::HttpReqExt};
use proton_core_api::{service::ApiServiceResult, services::proton::Proton};

pub const CALENDAR_V1: &str = "/calendar/v1";

pub trait ProtonCalendar {
    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/Calendar/operation/get_calendar-%7B_version%7D-%7BcalID%7D-bootstrap>
    fn get_calendar_bootstrap(
        &self,
        cal_id: &CalendarId,
    ) -> impl Future<Output = ApiServiceResult<CalendarBootstrap>> + Send;

    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/Event/operation/get_calendar-%7B_version%7D-%7BcalID%7D-events-%7BeventID%7D>
    fn get_calendar_event(
        &self,
        event_id: &CalendarEventId,
        recur_id: Option<&CalendarEventRecurrenceId>,
    ) -> impl Future<Output = ApiServiceResult<Option<CalendarEvent>>> + Send;
}

impl ProtonCalendar for Proton {
    async fn get_calendar_bootstrap(
        &self,
        cal_id: &CalendarId,
    ) -> ApiServiceResult<CalendarBootstrap> {
        Ok(GET!("{CALENDAR_V1}/{cal_id}/bootstrap")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_calendar_event(
        &self,
        event_id: &CalendarEventId,
        recur_id: Option<&CalendarEventRecurrenceId>,
    ) -> ApiServiceResult<Option<CalendarEvent>> {
        let req = GET!("{CALENDAR_V1}/events")
            .query(("UID", event_id))
            .query(("Page", 0))
            .query(("PageSize", 100))
            .query(("CalendarType", 0));

        let req = match recur_id {
            Some(id) => req.query(("RecurrenceID", id)),
            None => req,
        };

        let resp: FoundCalendarEvents = req.send_with(self).await?.ok()?.into_body_json()?;

        Ok(resp.events.into_iter().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions as pa;

    #[test]
    fn get_calendar_bootstrap() {
        let json = r##"
            {
                "Keys": [
                    {
                        "ID": "msY6Hh3D",
                        "PrivateKey": "-----BEGIN PGP PRIVATE KEY BLOCK----- ...",
                        "Flags": 3
                    }
                ],
                "Passphrase": {
                    "MemberPassphrases": [
                        {
                            "MemberID": "qyomV2nX",
                            "Passphrase": "-----BEGIN PGP MESSAGE----- ...",
                            "Signature": "-----BEGIN PGP SIGNATURE----- ..."
                        }
                    ]
                },
                "Members": [
                    {
                        "ID": "qyomV2nX",
                        "Name": "My calendar",
                        "Color": "#273EB2"
                    }
                ]
            }
        "##;

        let actual: CalendarBootstrap = serde_json::from_str(json).unwrap();

        let expected = CalendarBootstrap {
            keys: vec![CalendarKey {
                id: "msY6Hh3D".into(),
                private_key: "-----BEGIN PGP PRIVATE KEY BLOCK----- ...".into(),
                flags: CalendarKeyFlags::ActiveAndPrimary,
            }],
            passphrase: CalendarPassphrase {
                member_passphrases: vec![CalendarMemberPassphrase {
                    member_id: "qyomV2nX".into(),
                    passphrase: "-----BEGIN PGP MESSAGE----- ...".into(),
                    signature: "-----BEGIN PGP SIGNATURE----- ...".into(),
                }],
            },
            members: [CalendarMember {
                id: "qyomV2nX".into(),
                name: "My calendar".into(),
                color: "#273EB2".into(),
            }],
        };

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn get_calendar_event() {
        let json = r#"
            {
                "Code": 1000,
                "Events": [
                    {
                        "SharedEvents": [
                            {
                                "Type": 2,
                                "Data": "BEGIN:VCALENDAR..."
                            },
                            {
                                "Type": 3,
                                "Data": "0sBEASBA..."
                            }
                        ],
                        "CalendarID": "HzNtbT1J...",
                        "StartTime": 1744790400,
                        "EndTime": 1744792200,
                        "FullDay": 0,
                        "RecurrenceID": null,
                        "AddressKeyPacket": "wV4DkxOc...",
                        "SharedKeyPacket": null,
                        "AttendeesEvents": [
                            {
                                "Type": 3,
                                "Data": "0sLJAdwR..."
                            }
                        ],
                        "Attendees": [
                            {
                                "ID": "0FcDfeKS...",
                                "Token": "66791e2f...",
                                "Status": 0
                            }
                        ]
                    }
                ]
            }
        "#;

        let actual: FoundCalendarEvents = serde_json::from_str(json).unwrap();

        let expected = FoundCalendarEvents {
            events: vec![CalendarEvent {
                shared_events: vec![
                    CalendarEventPayload {
                        ty: CalendarEventPayloadType::Signed,
                        data: "BEGIN:VCALENDAR...".into(),
                    },
                    CalendarEventPayload {
                        ty: CalendarEventPayloadType::EncryptedAndSigned,
                        data: "0sBEASBA...".into(),
                    },
                ],
                calendar_id: "HzNtbT1J...".into(),
                start_time: 1_744_790_400,
                end_time: 1_744_792_200,
                full_day: false,
                recurrence_id: None,
                address_key_packet: Some("wV4DkxOc...".into()),
                shared_key_packet: None,
                attendees_events: vec![CalendarEventPayload {
                    ty: CalendarEventPayloadType::EncryptedAndSigned,
                    data: "0sLJAdwR...".into(),
                }],
                attendees: vec![CalendarAttendee {
                    id: "0FcDfeKS...".into(),
                    token: "66791e2f...".into(),
                    status: CalendarAttendeeStatus::Unanswered,
                }],
            }],
        };

        pa::assert_eq!(expected, actual);
    }
}
