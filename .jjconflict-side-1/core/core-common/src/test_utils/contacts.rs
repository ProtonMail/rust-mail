use crate::test_utils::test_context::TestContext;
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
use wiremock::MockBuilder;
use wiremock::Times;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_json, method, path},
};

impl TestContext {
    /// Generate new mock for retrieving contacts without emails and cards from the API.
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

    /// Generate new mock expectations for retrieving contacts.
    ///
    /// This function will mock the response for the given contacts.
    ///
    #[function_name::named]
    pub async fn mock_get_contacts(
        &self,
        contacts: Option<Vec<ApiContactBasic>>,
        expect: impl Into<Times>,
    ) {
        let contacts = contacts.unwrap_or_default();
        Mock::given(method("GET"))
            .and(path("/api/contacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetContactsResponse {
                    total: contacts.len() as u64,
                    contacts,
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    #[function_name::named]
    pub async fn mock_get_contacts_respond_with(&self, respond_with: impl Fn(MockBuilder) -> Mock) {
        let mock = Mock::given(method("GET")).and(path("/api/contacts"));
        let mock = respond_with(mock);
        mock.named(function_name!()).mount(self.mock_server()).await;
    }

    /// Generate new mock expectations for retrieving contact emails.
    ///
    /// This function will mock the response for the given contact emails.
    ///
    #[function_name::named]
    pub async fn mock_get_contacts_emails(
        &self,
        contact_emails: Option<Vec<ApiContactEmail>>,
        expect: impl Into<Times>,
    ) {
        let contact_emails = contact_emails.unwrap_or_default();
        Mock::given(method("GET"))
            .and(path("/api/contacts/emails"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetContactsEmailsResponse {
                    total: contact_emails.len() as u64,
                    contact_emails,
                }),
            )
            .expect(expect)
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }

    /// Generate new mock for retrieving contacts without emails and cards from the API.
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
