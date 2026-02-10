use crate::test_utils::test_context::TestContext;
use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::PatchLabelRequest;
use proton_core_api::services::proton::{GetLabelsResponse, PatchLabelResponse};
use wiremock::MockBuilder;
use wiremock::Times;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl TestContext {
    // Gets 3 labels called Label1, Label2, Label3
    #[function_name::named]
    pub async fn mock_get_all_labels(&self, labels: Vec<ApiLabel>) {
        let response = GetLabelsResponse { labels };

        Mock::given(method("GET"))
            .and(path("/api/core/v4/labels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_labels_by_ids(&self, labels: Vec<ApiLabel>) {
        let response = GetLabelsResponse { labels };

        Mock::given(method("POST"))
            .and(path("/api/core/v4/labels/by-ids"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_patch_label(&self, label_id: LabelId, expand: bool) {
        let request = PatchLabelRequest {
            expanded: Some(expand),
            notify: None,
        };
        let response = PatchLabelResponse {
            label: ApiLabel {
                expanded: expand,
                id: label_id.clone(),
                ..ApiLabel::test_default()
            },
        };
        Mock::given(method("PATCH"))
            .and(path(format!("/api/core/v4/labels/{label_id}")))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_labels_and(
        &self,
        labels: Vec<ApiLabel>,
        fun: impl Fn(MockBuilder) -> MockBuilder,
        expect: impl Into<Times>,
    ) {
        let response = GetLabelsResponse { labels };

        fun(Mock::given(method("GET")).and(path("/api/core/v4/labels")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}
