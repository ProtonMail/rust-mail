use proton_core_api::services::proton::ContactId;
use proton_core_api::services::proton::{
    ContactBasic as ApiContactBasic, ContactEmail as ApiContactEmail, ContactSendingPreferences,
};
use proton_core_common::datatypes::{
    AvatarInformation, ContactEmailItem, ContactItem, ContactItemType, GroupedContacts,
};
use proton_core_common::models::{Contact, ModelIdExtension};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;

#[tokio::test]
async fn contact_list() {
    let ctx = MailTestContext::new().await;
    let mut params = TestParams::default_basic();

    params.contacts = vec![ApiContactBasic {
        id: "123".into(),
        name: "Mr Banksy".to_string(),
        uid: "123".into(),

        create_time: 0,
        label_ids: vec![],
        modify_time: 0,
        size: 0,
    }];

    params.emails = vec![ApiContactEmail {
        id: "321".into(),
        contact_id: "123".into(),
        email: "banksy@proton.me".into(),
        name: "Mr Banksy".to_string(),
        canonical_email: "".into(),

        contact_type: vec![],
        defaults: ContactSendingPreferences::Default,
        is_proton: true,
        label_ids: vec![],
        last_used_time: 0,
        order: 0,
    }];

    ctx.setup_user(params.clone()).await;

    // Initialize Mocking
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let contact_list = Contact::contact_list(&tether).await.unwrap();

    assert_eq!(contact_list.len(), 1);
    assert_eq!(
        contact_list,
        vec![GroupedContacts {
            grouped_by: "M".to_string(),
            items: vec![ContactItemType::Contact(ContactItem {
                local_id: 1.into(),
                name: "Mr Banksy".to_string(),
                avatar_information: AvatarInformation {
                    text: "M".to_string(),
                    color: "#52CD96".to_string()
                },
                emails: vec![ContactEmailItem {
                    name: "Mr Banksy".into(),
                    avatar_information: AvatarInformation {
                        text: "M".to_string(),
                        color: "#52CD96".to_string()
                    },
                    local_contact_id: 1.into(),
                    email: "banksy@proton.me".into(),
                    is_proton: true,
                    last_used_time: 0.into()
                }]
            })]
        }]
    );
}

#[tokio::test]
async fn delete_contacts() {
    let ctx = MailTestContext::new().await;
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
        canonical_email: "".into(),
        contact_type: vec![],
        defaults: ContactSendingPreferences::Default,
        email: "banksy@proton.me".into(),
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
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let contact = Contact::find_by_remote_id(ContactId::from("123"), &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(!contact.deleted);

    Contact::action_delete(user_ctx.action_queue(), vec![contact.id()])
        .await
        .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    let contact = Contact::find_by_remote_id(ContactId::from("123"), &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(contact.deleted);

    let contact_list = Contact::contact_list(&tether).await.unwrap();
    assert_eq!(contact_list.len(), 0);
}
