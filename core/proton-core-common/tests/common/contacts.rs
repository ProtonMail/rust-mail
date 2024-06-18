use proton_api_core::{
    domain::{Contact, ContactEmail},
    requests::{GetAllContactsPartialResponse, GetContactEmailsResponse, GetFullContactResponse},
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

use super::TestContext;

impl TestContext {
    /// Generate new mock for retrieving contacts without emails and cards from the API.
    ///
    /// # Parameters
    ///
    /// * `contacts` - The contacts that should be in the mocked return.
    ///
    pub async fn mock_get_all_contacts_partial_request(&self, contacts: Vec<Contact>) {
        let num_contacts = contacts.len() as u64;
        let response = GetAllContactsPartialResponse {
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
    pub async fn mock_get_all_contact_emails_request(&self, contact_emails: Vec<ContactEmail>) {
        let num_contacts = contact_emails.len() as u64;
        let response = GetContactEmailsResponse {
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
    pub async fn mock_get_full_contact(&self, contact: Contact) {
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/contacts/{}",
                &contact.remote_id.as_ref().unwrap()
            )))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetFullContactResponse { contact }),
            )
            .expect(1)
            .mount(self.mock_server())
            .await;
    }
}
