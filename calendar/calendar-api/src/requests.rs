use crate::{CalendarAttendeeStatus, CalendarColor, CalendarNotification};
use serde::{Serialize, Serializer, ser::SerializeMap};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateCalendarEventAttendee {
    pub status: CalendarAttendeeStatus,
    pub update_time: i64,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpgradeCalendarEventInvite {
    pub shared_key_packet: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateCalendarEventPersonalPart {
    pub color: Option<CalendarColor>,
    pub notifications: CalendarNotificationsUpdate,
}

impl Serialize for UpdateCalendarEventPersonalPart {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = s.serialize_map(None)?;

        s.serialize_entry("Color", &self.color)?;

        match &self.notifications {
            CalendarNotificationsUpdate::Skip => {
                //
            }
            CalendarNotificationsUpdate::SetTo(notifications) => {
                s.serialize_entry("Notifications", notifications)?;
            }
            CalendarNotificationsUpdate::SetToDefault => {
                s.serialize_entry("Notifications", &Option::<Vec<CalendarNotification>>::None)?;
            }
        }

        s.end()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CalendarNotificationsUpdate {
    Skip,
    SetTo(Vec<CalendarNotification>),
    SetToDefault,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CalendarNotificationType;
    use indoc::indoc;
    use pretty_assertions as pa;

    #[test]
    fn update_calendar_event_personal_part() {
        let actual = serde_json::to_string_pretty(&UpdateCalendarEventPersonalPart {
            color: None,
            notifications: CalendarNotificationsUpdate::Skip,
        })
        .unwrap();

        let expected = indoc! {r#"
          {
            "Color": null
          }
        "#};

        pa::assert_eq!(expected.trim(), actual);

        // ---

        let actual = serde_json::to_string_pretty(&UpdateCalendarEventPersonalPart {
            color: Some(CalendarColor::new("#cafe00")),
            notifications: CalendarNotificationsUpdate::SetTo(vec![CalendarNotification {
                ty: CalendarNotificationType::Push,
                trigger: "-PT15M".parse().unwrap(),
            }]),
        })
        .unwrap();

        let expected = indoc! {r##"
          {
            "Color": "#cafe00",
            "Notifications": [
              {
                "Type": 1,
                "Trigger": "-PT15M"
              }
            ]
          }
        "##};

        pa::assert_eq!(expected.trim(), actual);

        // ---

        let actual = serde_json::to_string_pretty(&UpdateCalendarEventPersonalPart {
            color: None,
            notifications: CalendarNotificationsUpdate::SetToDefault,
        })
        .unwrap();

        let expected = indoc! {r#"
          {
            "Color": null,
            "Notifications": null
          }
        "#};

        pa::assert_eq!(expected.trim(), actual);
    }
}
