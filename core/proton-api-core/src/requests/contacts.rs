use crate::domain::{Contact, ContactEmail, ContactFilter, ContactId};
use crate::http::{JsonResponse, Method, RequestData, RequestDesc};
use serde::Serialize;
use serde::{self, Deserialize};

#[derive(Debug, Default)]
pub struct GetAllContactsPartialRequest {
    contact_filter: ContactFilter,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetAllContactsPartialResponse {
    pub contacts: Vec<Contact>,
    pub total: u64,
}

impl GetAllContactsPartialRequest {
    #[must_use]
    pub fn new(contact_filter: ContactFilter) -> Self {
        Self { contact_filter }
    }
}

impl RequestDesc for GetAllContactsPartialRequest {
    type Response = JsonResponse<GetAllContactsPartialResponse>;

    fn build(&self) -> RequestData {
        let mut request = RequestData::new(Method::Get, "contacts")
            .query("PageSize", &self.contact_filter.page_size)
            .query("Page", &self.contact_filter.page);
        if let Some(label_id) = &self.contact_filter.label_id {
            request = request.query("LabelID", label_id);
        };
        request
    }
}

#[derive(Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct GetFullContactRequest {
    id: ContactId,
}

impl GetFullContactRequest {
    #[must_use]
    pub fn new(id: ContactId) -> GetFullContactRequest {
        GetFullContactRequest { id }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetFullContactResponse {
    pub contact: Contact,
}

impl RequestDesc for GetFullContactRequest {
    type Response = JsonResponse<GetFullContactResponse>;

    fn build(&self) -> RequestData {
        RequestData::new(Method::Get, format!("contacts/{}", self.id))
    }
}

#[derive(Debug, Default)]
pub struct GetContactEmailsRequest {
    contact_filter: ContactFilter,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetContactEmailsResponse {
    pub contact_emails: Vec<ContactEmail>,
    pub total: u64,
}

impl GetContactEmailsRequest {
    #[must_use]
    pub fn new(contact_filter: ContactFilter) -> Self {
        Self { contact_filter }
    }
}

impl RequestDesc for GetContactEmailsRequest {
    type Response = JsonResponse<GetContactEmailsResponse>;

    fn build(&self) -> RequestData {
        let mut request = RequestData::new(Method::Get, "contacts/emails")
            .query("PageSize", &self.contact_filter.page_size)
            .query("Page", &self.contact_filter.page);
        if let Some(email) = &self.contact_filter.email {
            request = request.query("Email", email);
        };
        if let Some(label_id) = &self.contact_filter.label_id {
            request = request.query("LabelID", label_id);
        };
        request
    }
}
