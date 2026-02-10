use crate::{
    CalendarAttendeeStatus, CalendarBootstrap, CalendarEvent, CalendarNotificationsUpdate,
    FoundCalendarEvents, GetCalendarEvent, UpdateCalendarEventAttendee,
    UpdateCalendarEventPersonalPart,
};
use jiff::Zoned;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub trait ProtonCalendarMock {
    fn mock_get_calendar_bootstrap(
        &self,
        cal_id: &str,
        response: CalendarBootstrap,
    ) -> impl Future<Output = ()> + Send {
        self.mock_get_calendar_bootstrap_ex(cal_id, response, |mock| mock.expect(1))
    }

    fn mock_get_calendar_bootstrap_ex(
        &self,
        cal_id: &str,
        response: CalendarBootstrap,
        f: impl FnOnce(Mock) -> Mock + Send,
    ) -> impl Future<Output = ()> + Send;

    fn mock_get_calendar_event(
        &self,
        cal_id: &str,
        event_id: &str,
        event: CalendarEvent,
    ) -> impl Future<Output = ()> + Send;

    fn mock_find_calendar_events(
        &self,
        uid: &str,
        rid: Option<i64>,
        events: Vec<CalendarEvent>,
    ) -> impl Future<Output = ()> + Send;

    fn mock_upgrade_calendar_event_invite(
        &self,
        cal_id: &str,
        event_id: &str,
    ) -> impl Future<Output = ()> + Send;

    fn mock_update_calendar_event_attendee_status(
        &self,
        cal_id: &str,
        event_id: &str,
        att_id: &str,
        status: CalendarAttendeeStatus,
        update_time: &Zoned,
    ) -> impl Future<Output = ()> + Send;

    fn mock_update_calendar_event_personal_part(
        &self,
        cal_id: &str,
        event_id: &str,
        color: Option<&str>,
        notifications: CalendarNotificationsUpdate,
    ) -> impl Future<Output = ()> + Send;
}

impl ProtonCalendarMock for MockServer {
    #[function_name::named]
    async fn mock_get_calendar_bootstrap_ex(
        &self,
        cal_id: &str,
        response: CalendarBootstrap,
        f: impl FnOnce(Mock) -> Mock + Send,
    ) {
        let mock = Mock::given(method("GET"))
            .and(path(format!("/api/calendar/v1/{cal_id}/bootstrap")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!());

        f(mock).mount(self).await;
    }

    #[function_name::named]
    async fn mock_get_calendar_event(&self, cal_id: &str, event_id: &str, event: CalendarEvent) {
        let response = GetCalendarEvent { event };

        Mock::given(method("GET"))
            .and(path(format!("/api/calendar/v1/{cal_id}/events/{event_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self)
            .await;
    }

    #[function_name::named]
    async fn mock_find_calendar_events(
        &self,
        uid: &str,
        rid: Option<i64>,
        events: Vec<CalendarEvent>,
    ) {
        let response = FoundCalendarEvents { events };

        let mock = Mock::given(method("GET"))
            .and(path("/api/calendar/v1/events"))
            .and(query_param("UID", uid));

        let mock = match rid {
            Some(id) => mock.and(query_param("RecurrenceID", id.to_string())),
            None => mock,
        };

        mock.respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self)
            .await;
    }

    #[function_name::named]
    async fn mock_upgrade_calendar_event_invite(&self, cal_id: &str, event_id: &str) {
        Mock::given(method("PUT"))
            .and(path(format!(
                "/api/calendar/v1/{cal_id}/events/{event_id}/upgrade"
            )))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .named(function_name!())
            .mount(self)
            .await;
    }

    #[function_name::named]
    async fn mock_update_calendar_event_attendee_status(
        &self,
        cal_id: &str,
        event_id: &str,
        att_id: &str,
        status: CalendarAttendeeStatus,
        update_time: &Zoned,
    ) {
        Mock::given(method("PUT"))
            .and(path(format!(
                "/api/calendar/v1/{cal_id}/events/{event_id}/attendees/{att_id}"
            )))
            .and(body_json(UpdateCalendarEventAttendee {
                status,
                update_time: update_time.timestamp().as_second(),
                comment: None,
            }))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .named(function_name!())
            .mount(self)
            .await;
    }

    #[function_name::named]
    async fn mock_update_calendar_event_personal_part(
        &self,
        cal_id: &str,
        event_id: &str,
        color: Option<&str>,
        notifications: CalendarNotificationsUpdate,
    ) {
        Mock::given(method("PUT"))
            .and(path(format!(
                "/api/calendar/v1/{cal_id}/events/{event_id}/personal"
            )))
            .and(body_json(UpdateCalendarEventPersonalPart {
                color: color.map(Into::into),
                notifications,
            }))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .named(function_name!())
            .mount(self)
            .await;
    }
}
