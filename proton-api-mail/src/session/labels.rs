use super::MailSession;
use crate::domain::{Label, LabelId, LabelType};
use crate::requests::{
    CreateLabelRequest, DeleteLabelRequest, GetLabelsRequest, UpdateLabelRequest,
};
use proton_api_core::http;

impl MailSession {
    pub async fn labels(
        &self,
        label_type: LabelType,
    ) -> Result<Vec<Label>, http::HttpRequestError> {
        self.session
            .execute_request(GetLabelsRequest::new(label_type))
            .await
            .map(|r| r.labels)
    }

    pub async fn create_label(
        &self,
        name: &str,
        color: &str,
        label_type: LabelType,
        parent_id: Option<&LabelId>,
    ) -> Result<Label, http::HttpRequestError> {
        self.session
            .execute_request(CreateLabelRequest::new(name, color, label_type, parent_id))
            .await
            .map(|v| v.label)
    }

    pub async fn update_label(
        &self,
        id: &LabelId,
        name: &str,
        color: &str,
        parent_id: Option<&LabelId>,
    ) -> Result<Label, http::HttpRequestError> {
        self.session
            .execute_request(UpdateLabelRequest::new(id, name, color, parent_id))
            .await
            .map(|v| v.label)
    }

    pub async fn delete_label(&self, parent_id: &LabelId) -> Result<(), http::HttpRequestError> {
        self.session
            .execute_request(DeleteLabelRequest::new(parent_id))
            .await
    }
}
