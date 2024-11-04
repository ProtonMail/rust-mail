use proton_api_core::services::proton::response_data::{
    ContactBasic as ApiContactBasic, ContactEmail as ApiContactEmail, ContactSendingPreferences,
};
use proton_api_mail::session;
use proton_core_common::datatypes::{
    AvatarInformation, ContactEmailItem, ContactItem, ContactItemType, GroupedContacts, RemoteId,
};
use proton_core_common::models::{Contact, ModelExtension};
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::test_context::MailTestContext;

#[tokio::test]
async fn contact_list() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();
    let mut params = TestParams::default_basic();

    params.contacts = vec![ApiContactBasic {
        id: "123".into(),
        create_time: 0,
        label_ids: vec![],
        modify_time: 0,
        name: "Mr Banksy".to_string(),
        size: 0,
        uid: "123".into(),
    }];

    params.emails = vec![ApiContactEmail {
        id: "321".into(),
        contact_id: "123".into(),
        canonical_email: "".to_string(),
        contact_type: vec![],
        defaults: ContactSendingPreferences::Default,
        email: "banksy@proton.me".to_string(),
        is_proton: true,
        label_ids: vec![],
        last_used_time: 0,
        name: "Mr Banksy".to_string(),
        order: 0,
    }];

    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.catch_all().await;

    ctx.init_user(user_ctx.clone()).await;

    let contact_list = Contact::contact_list(stash).await.unwrap();

    assert_eq!(contact_list.len(), 1);
    assert_eq!(
        contact_list,
        vec![GroupedContacts {
            grouped_by: "M".to_string(),
            item: vec![ContactItemType::Contact(ContactItem {
                local_id: 1.into(),
                name: "Mr Banksy".to_string(),
                avatar_information: AvatarInformation {
                    text: "MB".to_string(),
                    color: "#1ED19C".to_string()
                },
                emails: vec![ContactEmailItem {
                    local_id: 1.into(),
                    email: "banksy@proton.me".to_string()
                }]
            })]
        }]
    );
}

#[tokio::test]
async fn delete_contacts() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let stash = user_ctx.user_stash();
    let mut params = TestParams::default_basic();

    params.contacts = vec![ApiContactBasic {
        id: "123".into(),
        create_time: 0,
        label_ids: vec![],
        modify_time: 0,
        name: "Mr Banksy".to_string(),
        size: 0,
        uid: "123".into(),
    }];

    params.emails = vec![ApiContactEmail {
        id: "321".into(),
        contact_id: "123".into(),
        canonical_email: "".to_string(),
        contact_type: vec![],
        defaults: ContactSendingPreferences::Default,
        email: "banksy@proton.me".to_string(),
        is_proton: true,
        label_ids: vec![],
        last_used_time: 0,
        name: "Mr Banksy".to_string(),
        order: 0,
    }];

    ctx.setup_user(params.clone()).await;
    ctx.core_test_context
        .mock_delete_contacts(vec!["123".into()])
        .await;

    // Initialize Mocking
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let contact = Contact::find_by_id(RemoteId::from("123"), stash)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(contact.deleted, false);

    let queue = user_ctx.queue();
    let session = user_ctx.session();

    Contact::action_delete(session, queue, vec![contact.local_id.unwrap()])
        .await
        .unwrap();

    let contact = Contact::find_by_id(RemoteId::from("123"), stash)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(contact.deleted, true);

    let contact_list = Contact::contact_list(stash).await.unwrap();
    assert_eq!(contact_list.len(), 0);
}
