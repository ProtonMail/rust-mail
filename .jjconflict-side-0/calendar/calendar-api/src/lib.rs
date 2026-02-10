mod common;
mod requests;
mod responses;

#[cfg(feature = "test-utils")]
mod test_utils;

pub use self::common::*;
pub use self::requests::*;
pub use self::responses::*;

#[cfg(feature = "test-utils")]
pub use self::test_utils::*;

use jiff::Zoned;
use muon::PUT;
use muon::ProtonRequest;
use muon::ProtonResponse;
use muon::common::Sender;
use muon::{GET, http::HttpReqExt};
use proton_core_api::service::ApiServiceResult;

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
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
    ) -> impl Future<Output = ApiServiceResult<CalendarEvent>> + Send;

    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/Event/operation/get_calendar-%7B_version%7D-events>
    fn find_calendar_events(
        &self,
        uid: &CalendarEventUid,
        rid: Option<CalendarEventRecurrenceId>,
    ) -> impl Future<Output = ApiServiceResult<Vec<CalendarEvent>>> + Send;

    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/VTimezone/operation/get_calendar-%7B_version%7D-vtimezones>
    ///
    /// Requires `timezones.len() <= 10`.
    fn get_calendar_vtimezones(
        &self,
        timezones: &[&str],
    ) -> impl Future<Output = ApiServiceResult<CalendarVTimezones>>;

    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/Event/operation/put_calendar-%7B_version%7D-%7BcalID%7D-events-%7BeventID%7D-upgrade>
    fn upgrade_calendar_event_invite(
        &self,
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
        shared_key_packet: &str,
    ) -> impl Future<Output = ApiServiceResult<()>> + Send;

    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/Event/operation/put_calendar-%7B_version%7D-%7BcalID%7D-events-%7BeventID%7D-attendees-%7BattendeeID%7D>
    fn update_calendar_event_attendee_status(
        &self,
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
        att_id: &CalendarAttendeeId,
        status: CalendarAttendeeStatus,
        update_time: &Zoned,
    ) -> impl Future<Output = ApiServiceResult<()>> + Send;

    /// <https://protonmail.gitlab-pages.protontech.ch/Slim-API/calendar/#tag/Event/operation/put_calendar-%7B_version%7D-%7BcalID%7D-events-%7BeventID%7D-personal>
    fn update_calendar_event_personal_part(
        &self,
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
        color: Option<CalendarColor>,
        notifications: CalendarNotificationsUpdate,
    ) -> impl Future<Output = ApiServiceResult<()>> + Send;
}

impl<This: ?Sized + Sender<ProtonRequest, ProtonResponse>> ProtonCalendar for This {
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
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
    ) -> ApiServiceResult<CalendarEvent> {
        let resp: GetCalendarEvent = GET!("{CALENDAR_V1}/{cal_id}/events/{event_id}")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?;

        Ok(resp.event)
    }

    async fn find_calendar_events(
        &self,
        uid: &CalendarEventUid,
        rid: Option<CalendarEventRecurrenceId>,
    ) -> ApiServiceResult<Vec<CalendarEvent>> {
        let req = GET!("{CALENDAR_V1}/events")
            .query(("UID", uid))
            .query(("Page", 0))
            .query(("PageSize", 100))
            .query(("CalendarType", 0));

        let req = match rid {
            Some(id) => req.query(("RecurrenceID", id.get())),
            None => req,
        };

        let resp: FoundCalendarEvents = req.send_with(self).await?.ok()?.into_body_json()?;

        Ok(resp.events)
    }

    async fn get_calendar_vtimezones(
        &self,
        timezones: &[&str],
    ) -> ApiServiceResult<CalendarVTimezones> {
        let mut req = GET!("{CALENDAR_V1}/vtimezones");

        for timezone in timezones {
            req = req.query(("Timezones[]", timezone));
        }

        req.send_with(self)
            .await?
            .ok()?
            .body_json()
            .map_err(Into::into)
    }

    async fn upgrade_calendar_event_invite(
        &self,
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
        shared_key_packet: &str,
    ) -> ApiServiceResult<()> {
        PUT!("{CALENDAR_V1}/{cal_id}/events/{event_id}/upgrade")
            .body_json(UpgradeCalendarEventInvite {
                shared_key_packet: shared_key_packet.into(),
            })?
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }

    async fn update_calendar_event_attendee_status(
        &self,
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
        att_id: &CalendarAttendeeId,
        status: CalendarAttendeeStatus,
        update_time: &Zoned,
    ) -> ApiServiceResult<()> {
        PUT!("{CALENDAR_V1}/{cal_id}/events/{event_id}/attendees/{att_id}")
            .body_json(UpdateCalendarEventAttendee {
                status,
                update_time: update_time.timestamp().as_second(),
                comment: None,
            })?
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }

    async fn update_calendar_event_personal_part(
        &self,
        cal_id: &CalendarId,
        event_id: &CalendarEventId,
        color: Option<CalendarColor>,
        notifications: CalendarNotificationsUpdate,
    ) -> ApiServiceResult<()> {
        PUT!("{CALENDAR_V1}/{cal_id}/events/{event_id}/personal")
            .body_json(UpdateCalendarEventPersonalPart {
                color,
                notifications,
            })?
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
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
                        "Color": "#273EB2",
                        "AddressID": "pTF47iZy"
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
                address_id: "pTF47iZy".into(),
            }],
        };

        pa::assert_eq!(expected, actual);
    }

    #[allow(clippy::too_many_lines)]
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
                                "Data": "BEGIN:VCALENDAR...",
                                "Signature": "-----BEGIN PGP SIGNATURE-----...",
                                "Author": "spongebob@squarepants.com"
                            },
                            {
                                "Type": 3,
                                "Data": "0sBEASBA...",
                                "Signature": "-----BEGIN PGP SIGNATURE-----...",
                                "Author": "spongebob@squarepants.com"
                            }
                        ],
                        "CalendarEvents": [
                            {
                                "Type": 0,
                                "Data": "BEGIN:VCALENDAR...",
                                "Signature": null,
                                "Author": "spongebob@squarepants.com"
                            }
                        ],
                        "ID": "6GAnNerJ...",
                        "AddressID": "ofMToh8I...",
                        "CalendarID": "HzNtbT1J...",
                        "StartTime": 1744790400,
                        "EndTime": 1744792200,
                        "FullDay": 0,
                        "RecurrenceID": 1744792300,
                        "AddressKeyPacket": "wV4DkxOc...",
                        "SharedKeyPacket": null,
                        "AttendeesEvents": [
                            {
                                "Type": 3,
                                "Data": "0sLJAdwR...",
                                "Signature": "-----BEGIN PGP SIGNATURE-----...",
                                "Author": "spongebob@squarepants.com"
                            }
                        ],
                        "Attendees": [
                            {
                                "ID": "0FcDfeKS...",
                                "Token": "66791e2f...",
                                "Status": 0
                            }
                        ],
                        "Notifications": [
                            {
                                "Type": 1,
                                "Trigger": "-PT15M"
                            }
                        ],
                        "Color": null,
                        "IsProtonProtonInvite": 1
                    }
                ]
            }
        "#;

        let actual: FoundCalendarEvents = serde_json::from_str(json).unwrap();

        let expected = FoundCalendarEvents {
            events: vec![CalendarEvent {
                id: "6GAnNerJ...".into(),
                address_id: Some("ofMToh8I...".into()),
                shared_events: vec![
                    CalendarEventPayload {
                        ty: CalendarEventPayloadType::Signed,
                        data: "BEGIN:VCALENDAR...".into(),
                        signature: Some("-----BEGIN PGP SIGNATURE-----...".into()),
                        author: "spongebob@squarepants.com".into(),
                    },
                    CalendarEventPayload {
                        ty: CalendarEventPayloadType::EncryptedAndSigned,
                        data: "0sBEASBA...".into(),
                        signature: Some("-----BEGIN PGP SIGNATURE-----...".into()),
                        author: "spongebob@squarepants.com".into(),
                    },
                ],
                calendar_events: vec![CalendarEventPayload {
                    ty: CalendarEventPayloadType::ClearText,
                    data: "BEGIN:VCALENDAR...".into(),
                    signature: None,
                    author: "spongebob@squarepants.com".into(),
                }],
                calendar_id: "HzNtbT1J...".into(),
                address_key_packet: Some("wV4DkxOc...".into()),
                shared_key_packet: None,
                attendees_events: vec![CalendarEventPayload {
                    ty: CalendarEventPayloadType::EncryptedAndSigned,
                    data: "0sLJAdwR...".into(),
                    signature: Some("-----BEGIN PGP SIGNATURE-----...".into()),
                    author: "spongebob@squarepants.com".into(),
                }],
                attendees: vec![CalendarAttendee {
                    id: "0FcDfeKS...".into(),
                    token: "66791e2f...".into(),
                    status: CalendarAttendeeStatus::Unanswered,
                }],
                notifications: Some(vec![CalendarNotification {
                    ty: CalendarNotificationType::Push,
                    trigger: "-PT15M".parse().unwrap(),
                }]),
                color: None,
                is_proton_proton_invite: true,
            }],
        };

        pa::assert_eq!(expected, actual);
    }
}
