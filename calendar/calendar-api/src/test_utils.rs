use crate::{CalendarBootstrap, CalendarEvent, FoundCalendarEvents};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub trait ProtonCalendarMock {
    fn mock_get_calendar_bootstrap(
        &self,
        uid: &str,
        response: CalendarBootstrap,
    ) -> impl Future<Output = ()> + Send;

    fn mock_get_calendar_event(
        &self,
        uid: &str,
        recurrence_id: Option<&str>,
        event: Option<CalendarEvent>,
    ) -> impl Future<Output = ()> + Send;
}

impl ProtonCalendarMock for MockServer {
    #[function_name::named]
    async fn mock_get_calendar_bootstrap(&self, uid: &str, response: CalendarBootstrap) {
        Mock::given(method("GET"))
            .and(path(format!("/api/calendar/v1/{uid}/bootstrap")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self)
            .await;
    }

    #[function_name::named]
    async fn mock_get_calendar_event(
        &self,
        uid: &str,
        recurrence_id: Option<&str>,
        event: Option<CalendarEvent>,
    ) {
        let response = FoundCalendarEvents {
            events: Vec::from_iter(event),
        };

        let mock = Mock::given(method("GET"))
            .and(path("/api/calendar/v1/events"))
            .and(query_param("UID", uid));

        let mock = match recurrence_id {
            Some(id) => mock.and(query_param("RecurrenceID", id)),
            None => mock,
        };

        mock.respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self)
            .await;
    }
}
