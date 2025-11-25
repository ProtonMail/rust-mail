use proton_core_api::services::proton::{EventId, GetEventsLatestResponse};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use super::test_context::TestContext;

impl TestContext {
    pub async fn mock_last_event_id(&self, id: EventId) {
        Mock::given(method("GET"))
            .and(path("/api/core/v4/events/latest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetEventsLatestResponse { event_id: id }),
            )
            .expect(1) // this should only ever be initialized once at the moment
            .named("Setup user get latest events")
            .mount(self.mock_server())
            .await;
    }
}
