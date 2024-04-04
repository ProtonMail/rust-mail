use crate::domain::{Label, LabelId, LabelType};
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::http;
use proton_api_core::http::{JsonResponse, NoResponse, RequestData};

pub struct GetLabelsRequest {
    label_type: LabelType,
}

#[doc(hidden)]
#[derive(Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetLabelsResponse {
    pub labels: Vec<Label>,
}

impl GetLabelsRequest {
    pub fn new(label_type: LabelType) -> Self {
        Self { label_type }
    }
}

impl http::RequestDesc for GetLabelsRequest {
    type Response = JsonResponse<GetLabelsResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Get, "core/v4/labels").query("Type", self.label_type as u8)
    }
}

#[doc(hidden)]
#[derive(Deserialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct CreateOrUpdateLabelResponse {
    pub label: Label,
}
#[doc(hidden)]
#[derive(Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct CreateLabelRequest<'a> {
    name: &'a str,
    color: &'a str,
    r#type: LabelType,
    #[serde(rename = "ParentID")]
    parent_id: Option<&'a LabelId>,
}

impl<'a> CreateLabelRequest<'a> {
    pub fn new(
        name: &'a str,
        color: &'a str,
        label_type: LabelType,
        parent_id: Option<&'a LabelId>,
    ) -> Self {
        Self {
            name,
            color,
            r#type: label_type,
            parent_id,
        }
    }
}

impl<'a> http::RequestDesc for CreateLabelRequest<'a> {
    type Response = JsonResponse<CreateOrUpdateLabelResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Post, "core/v4/labels").json(self)
    }
}

#[doc(hidden)]
#[derive(Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct UpdateLabelRequest<'a> {
    #[serde(skip)]
    id: &'a LabelId,
    name: &'a str,
    color: &'a str,
    #[serde(rename = "ParentID")]
    parent_id: Option<&'a LabelId>,
}

impl<'a> UpdateLabelRequest<'a> {
    pub fn new(
        id: &'a LabelId,
        name: &'a str,
        color: &'a str,
        parent_id: Option<&'a LabelId>,
    ) -> Self {
        Self {
            id,
            name,
            color,
            parent_id,
        }
    }
}

impl<'a> http::RequestDesc for UpdateLabelRequest<'a> {
    type Response = JsonResponse<CreateOrUpdateLabelResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Put, format!("core/v4/labels/{}", self.id)).json(self)
    }
}

#[doc(hidden)]
pub struct DeleteLabelRequest<'a> {
    id: &'a LabelId,
}

impl<'a> DeleteLabelRequest<'a> {
    pub fn new(id: &'a LabelId) -> Self {
        Self { id }
    }
}

impl<'a> http::RequestDesc for DeleteLabelRequest<'a> {
    type Response = NoResponse;

    fn build(&self) -> RequestData {
        RequestData::new(http::Method::Delete, format!("core/v4/labels/{}", self.id))
    }
}
