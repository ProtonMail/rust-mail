use crate::common::TestContext;
use proton_api_core::services::proton::common::RemoteId;
use proton_api_mail::services::proton::requests::PatchLabelRequest;
use proton_api_mail::services::proton::response_data::OperationResult;
use proton_api_mail::services::proton::responses::PatchLabelResponse;
use proton_core_common::datatypes::LabelId;
use proton_mail_common::actions::labels::Expand;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

impl TestContext {
    pub async fn mock_patch_label(&self, label_id: LabelId, expand: bool) {
        let request = PatchLabelRequest {
            expanded: Some(expand),
            notify: None,
        };
        let response = PatchLabelResponse {
            responses: vec![OperationResult {
                id: label_id.clone().into(),
                response: Default::default(),
            }],
        };
        Mock::given(method("PATCH"))
            .and(path(format!("/api/core/v4/labels/{}", label_id)))
            .and(body_json(request))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}
