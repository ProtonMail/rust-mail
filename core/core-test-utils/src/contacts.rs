use crate::test_context::TestContext;
use proton_api_core::services::proton::response_data::{
    ContactBasic as ApiContactBasic, ContactEmail as ApiContactEmail, ContactFull as ApiContactFull,
};
use proton_api_core::services::proton::responses::{
    GetContactResponse, GetContactsEmailsResponse, GetContactsResponse,
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

impl TestContext {
    /// Generate new mock for retrieving contacts without emails and cards from the API.
    ///
    /// # Parameters
    ///
    /// * `contacts` - The contacts that should be in the mocked return.
    ///
    pub async fn mock_get_all_contacts_partial_request(&self, contacts: Vec<ApiContactBasic>) {
        let num_contacts = contacts.len() as u64;
        let response = GetContactsResponse {
            contacts,
            total: num_contacts,
        };
        Mock::given(method("GET"))
            .and(path(r"/api/contacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for retrieving contacts without emails and cards from the API.
    ///
    /// # Parameters
    ///
    /// * `contacts` - The contacts that should be in the mocked return.
    ///
    pub async fn mock_get_all_contact_emails_request(&self, contact_emails: Vec<ApiContactEmail>) {
        let num_contacts = contact_emails.len() as u64;
        let response = GetContactsEmailsResponse {
            contact_emails,
            total: num_contacts,
        };
        Mock::given(method("GET"))
            .and(path("/api/contacts/emails"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for retrieving a full contact with emails and cards from the API.
    ///
    /// # Parameters
    ///
    /// * `contacts` - The contacts that should be in the mocked return.
    ///
    pub async fn mock_get_full_contact(&self, contact: ApiContactFull) {
        Mock::given(method("GET"))
            .and(path(format!("/api/contacts/{}", &contact.id)))
            .respond_with(ResponseTemplate::new(200).set_body_json(GetContactResponse { contact }))
            //.expect(1)
            .mount(self.mock_server())
            .await;
    }
}
