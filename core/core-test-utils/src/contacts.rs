use crate::test_context::TestContext;
use proton_core_api::services::proton::ContactId;
use proton_core_api::services::proton::PutDeleteContacts;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_api::services::proton::{
    ContactBasic as ApiContactBasic, ContactEmail as ApiContactEmail, ContactFull as ApiContactFull,
};
use proton_core_api::services::proton::{
    GetContactResponse, GetContactsEmailsResponse, GetContactsResponse, PutDeleteContactResponse,
    PutDeleteContactsResponse,
};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_json, method, path},
};

impl TestContext {
    /// Generate new mock for retrieving contacts without emails and cards from the API.
    ///
    /// # Parameters
    ///
    /// * `contacts` - The contacts that should be in the mocked return.
    ///
    #[function_name::named]
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for retrieving contacts without emails and cards from the API.
    ///
    /// # Parameters
    ///
    /// * `contacts` - The contacts that should be in the mocked return.
    ///
    #[function_name::named]
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
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for retrieving a full contact with emails and cards from the API.
    ///
    /// # Parameters
    ///
    /// * `contacts` - The contacts that should be in the mocked return.
    ///
    #[function_name::named]
    pub async fn mock_get_full_contact(&self, contact: ApiContactFull) {
        Mock::given(method("GET"))
            .and(path(format!("/api/contacts/{}", &contact.id)))
            .respond_with(ResponseTemplate::new(200).set_body_json(GetContactResponse { contact }))
            //.expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for deleting contacts from the API.
    ///
    /// # Parameters
    ///
    /// * `contact_ids` - The contacts that should be delted.
    ///
    #[function_name::named]
    pub async fn mock_delete_contacts(&self, contact_ids: Vec<ContactId>) {
        Mock::given(method("PUT"))
            .and(path("/api/contacts/delete"))
            .and(body_json(PutDeleteContacts {
                ids: contact_ids.clone(),
            }))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(PutDeleteContactsResponse {
                    responses: contact_ids
                        .into_iter()
                        .map(|id| PutDeleteContactResponse {
                            id,
                            response: ApiErrorInfo {
                                code: 1000,
                                ..Default::default()
                            },
                        })
                        .collect(),
                }),
            )
            .expect(1)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}
