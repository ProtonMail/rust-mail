use crate::test_context::TestContext;
use proton_api_core::services::proton::common::RemoteId;
use proton_api_core::services::proton::response_data::ApiErrorInfo;
use proton_api_mail::services::proton::common::LabelType;
use proton_api_mail::services::proton::requests::{GetLabelsOptions, PatchLabelRequest};
use proton_api_mail::services::proton::response_data::{Label as ApiLabel, OperationResult};
use proton_api_mail::services::proton::responses::{GetLabelsResponse, PatchLabelResponse};
use proton_core_common::datatypes::LabelId;
use proton_mail_common::actions::labels::Expand;
use proton_mail_common::ALL_LABEL_TYPES;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl TestContext {
    // Gets 3 labels called Label1, Label2, Label3
    pub async fn mock_get_all_labels(&self, labels: Vec<ApiLabel>) {
        let response = GetLabelsResponse { labels };

        Mock::given(method("GET"))
            .and(path("/api/core/v4/labels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(self.mock_server())
            .await;
    }

    pub async fn mock_get_labels_by_ids(&self, labels: Vec<ApiLabel>) {
        let response = GetLabelsResponse { labels };

        Mock::given(method("POST"))
            .and(path("/api/core/v4/labels/by-ids"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(self.mock_server())
            .await;
    }

    pub async fn mock_patch_label(&self, label_id: LabelId, expand: bool) {
        let request = PatchLabelRequest {
            expanded: Some(expand),
            notify: None,
        };
        let response = PatchLabelResponse {
            responses: vec![OperationResult {
                id: label_id.clone().into(),
                response: ApiErrorInfo::default(),
            }],
        };
        Mock::given(method("PATCH"))
            .and(path(format!("/api/core/v4/labels/{label_id}")))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}
