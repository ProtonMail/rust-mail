use common::{TestContext, TestCoreEvent};
use proton_api_core::domain::{
    Action, CardData, CardSignature, CardType, Contact, ContactCard, ContactEmail,
    ContactEmailEvent, ContactEmailId, ContactEvent, ContactId, ContactLabelId,
    ContactSendingPreferences, ContactType, ContactUid,
};
use proton_core_common::{
    CoreEventSubscriber, UserContext,
};
use proton_event_loop::Subscriber;
use stash::orm::Model;
use stash::params;

mod common;

#[tokio::test]
async fn test_sync_and_load_contacts() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let test_contacts = create_test_partial_contacts();
    let test_contacts_email = create_test_contact_emails();

    // Api mock.
        ctx.mock_get_all_contacts_partial_request(test_contacts.clone())
            .await;
        ctx.mock_get_all_contact_emails_request(test_contacts_email.clone())
            .await;
        ctx.catch_all().await;

    // Sync contacts
    user_ctx.sync_contacts().await
        .expect("failed to sync contacts");

    // Check database
    let conn = user_ctx.stash();
    let contacts = Contact::find("LIMIT 100", vec![], &conn, None).await
        .expect("Failed to get contacts");
    let expected_contacts = expected_local_contacts();
    assert_eq!(contacts, expected_contacts);
}

#[tokio::test]
async fn test_sync_and_load_full_contact() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let test_full_contact = create_test_full_contact();

    // Api mock.
        ctx.mock_get_full_contact(test_full_contact.clone()).await;
        ctx.catch_all().await;

    // Sync contacts
    user_ctx.sync_contact_with_card(test_full_contact.remote_id).await
        .expect("failed to sync contacts");

    // Check database
    let conn = user_ctx.stash();
    let contact = Contact::load(1, &conn).await
        .expect("Failed to load contact")
        .expect("contact should be found");
    let expected_contact = create_test_full_contact();
    assert_eq!(contact, expected_contact);
}

#[tokio::test]
async fn test_sync_and_load_contacts_mixed() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let test_contacts = create_test_partial_contacts();
    let test_contacts_email = create_test_contact_emails();
    let test_full_contact = create_test_full_contact();
    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts.clone(),
        test_contacts_email.clone(),
        test_full_contact.clone(),
    ).await;

    // Check database
    let conn = user_ctx.stash();
    
    let contact = Contact::load(1, &conn).await
        .expect("Failed to load contact")
        .expect("contact should be found");
    let expected_contact = create_test_full_contact();
    assert_eq!(contact, expected_contact);

    let contacts = Contact::find("LIMIT 100", vec![], &conn, None).await
        .expect("Failed to load contacts");
    let expected_contacts = expected_local_contacts();
    assert_eq!(contacts, expected_contacts);

    let email_to_query = "contact_email_1@contact.test";
    let queried_contact_emails = ContactEmail::find("WHERE canonical_email = ?", params![email_to_query], &conn, None).await
        .expect("Failed to get contact emails");
    let expected_mail = contact
        .contact_emails
        .iter()
        .find(|email| email.canonical_email == email_to_query)
        .expect("expect to be found");
    assert_eq!(queried_contact_emails.first().unwrap(), expected_mail);
}

#[tokio::test]
async fn test_sync_and_delete_event_contact() {
    let ctx: TestContext = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let test_event_subscriber = CoreEventSubscriber::new(&ctx);

    let test_contacts = create_test_partial_contacts();
    let test_contacts_email = create_test_contact_emails();
    let test_full_contact = create_test_full_contact();
    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts.clone(),
        test_contacts_email.clone(),
        test_full_contact.clone(),
    ).await;

    let email_id = test_contacts_email.first().unwrap().clone();
    let contact_to_remove = test_contacts.last().unwrap();

    let delete_event = ContactEmailEvent {
        id: email_id.remote_id.clone(),
        action: Action::Delete,
        contact_email: None,
    };
    let delete_contact_event = ContactEvent {
        id: contact_to_remove.remote_id.clone(),
        action: Action::Delete,
        contact: None,
    };
    let events = TestCoreEvent {
        contact_emails: Some(vec![delete_event]),
        contacts: Some(vec![delete_contact_event]),
        ..Default::default()
    };
    // Fire event:
    test_event_subscriber.on_events(&mut [events]).await
        .expect("failed to execute event");

    // Were the  deletions successful?
    let conn = user_ctx.stash();
    let queried_contact_emails = ContactEmail::find("WHERE canonical_email = ?", params![email_id.canonical_email], &conn, None).await
        .expect("Failed to get contact emails");
    assert!(queried_contact_emails.is_empty());

    let contacts = Contact::find("LIMIT 100", vec![], &conn, None).await
        .expect("Failed to get contacts");
    assert_eq!(contacts.len(), test_contacts.len() - 1);
}

#[tokio::test]
async fn test_sync_and_modify_event_contact() {
    let ctx: TestContext = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let test_event_subscriber = CoreEventSubscriber::new(&ctx);

    let test_contacts = create_test_partial_contacts();
    let test_contacts_email = create_test_contact_emails();
    let test_full_contact = create_test_full_contact();
    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts.clone(),
        test_contacts_email.clone(),
        test_full_contact.clone(),
    ).await;

    let (modified_contact, removed_email) = create_test_full_modified_contact();

    let delete_contact_event = ContactEvent {
        id: modified_contact.remote_id.clone(),
        action: Action::Update,
        contact: Some(modified_contact.clone()),
    };
    let event = TestCoreEvent {
        contacts: Some(vec![delete_contact_event]),
        ..Default::default()
    };
    // Fire event:
    test_event_subscriber.on_events(&mut [event]).await
        .expect("failed to execute event");

    let conn = user_ctx.stash();
    let queried_contact_emails = ContactEmail::find("WHERE canonical_email = ?", params![removed_email.canonical_email], &conn, None).await
        .expect("Failed to get contact emails");
    assert!(queried_contact_emails.is_empty());

    let contact = Contact::load(1, &conn).await
        .expect("Failed to load contact")
        .expect("contact should be found");

    assert_eq!(
        contact.modify_time,
        modified_contact.modify_time
    );
    assert_eq!(contact.size, modified_contact.size);
    assert_eq!(
        contact.contact_emails.len(),
        modified_contact.contact_emails.len()
    );
    let expected_cards: Vec<ContactCard> = modified_contact
        .cards
        .iter()
        .map(|value| value.clone().into())
        .collect();
    assert_eq!(contact.cards, expected_cards);
}

async fn prepare_sync_test_data_contacts(
    ctx: &TestContext,
    user_ctx: &UserContext,
    test_contacts: Vec<Contact>,
    test_contacts_email: Vec<ContactEmail>,
    test_full_contact: Contact,
) {
    let contact_id = test_full_contact.remote_id.clone();
    // Api mock.
        ctx.mock_get_all_contacts_partial_request(test_contacts)
            .await;
        ctx.mock_get_all_contact_emails_request(test_contacts_email)
            .await;
        ctx.mock_get_full_contact(test_full_contact).await;
        ctx.catch_all().await;

    // Sync contacts
    user_ctx.sync_contacts().await
        .expect("failed to sync contacts");
    user_ctx.sync_contact_with_card(contact_id).await
        .expect("failed to sync contacts");
}

fn create_test_partial_contacts() -> Vec<Contact> {
    vec![
        Contact {
            id: None,
            remote_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            name: "contact_name".to_owned(),
            uid: ContactUid::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04"),
            size: 1443,
            create_time: 1_503_815_366,
            modify_time: 1_503_815_366,
            contact_emails: vec![],
            label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            cards: vec![],
            row_id: None,
            stash: None,
        },
        Contact {
            id: None,
            remote_id: ContactId::from("z29olIjFv0rnXxBhSMz=="),
            name: "contact_name2".to_owned(),
            uid: ContactUid::from("proton-legacy-139892c2-f691-4118-8c29-061196013e01"),
            size: 1445,
            create_time: 1_503_815_367,
            modify_time: 1_503_815_367,
            contact_emails: vec![],
            label_ids: vec![ContactLabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
            cards: vec![],
            row_id: None,
            stash: None,
        },
    ]
}

fn create_test_contact_emails() -> Vec<ContactEmail> {
    vec![
        ContactEmail {
            id: None,
            remote_id: ContactEmailId::from("aefew4323jFv0BhSMw=="),
            name: "contact_email_name_1".to_owned(),
            email: "contact_email_1@contact.test".to_owned(),
            contact_type: vec![ContactType::from("work")],
            defaults: ContactSendingPreferences::Default,
            display_order: 1,
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            label_ids: vec![ContactLabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
            canonical_email: "contact_email_1@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
            row_id: None,
            stash: None,
        },
        ContactEmail {
            id: None,
            remote_id: ContactEmailId::from("aefew4323jFv0BhSMz=="),
            name: "contact_email_name_2".to_owned(),
            email: "contact_email_2@contact.test".to_owned(),
            contact_type: vec![ContactType::from("work")],
            defaults: ContactSendingPreferences::Default,
            display_order: 1,
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            canonical_email: "contact_email_2@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
            row_id: None,
            stash: None,
        },
        ContactEmail {
            id: None,
            remote_id: ContactEmailId::from("oZfew4323jFv0BhSMz=="),
            name: "contact_email_name_3".to_owned(),
            email: "contact_email_3@contact.test".to_owned(),
            contact_type: vec![ContactType::from("work")],
            defaults: ContactSendingPreferences::Custom,
            display_order: 1,
            contact_id: ContactId::from("z29olIjFv0rnXxBhSMz=="),
            label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            canonical_email: "contact_email_3@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
            row_id: None,
            stash: None,
        },
    ]
}

fn expected_local_contacts() -> Vec<Contact> {
    let contacts = create_test_partial_contacts();
    let contact_emails = create_test_contact_emails();
    contacts
        .into_iter()
        .enumerate()
        .map(|(_index, mut contact)| {
            let contact_emails: Vec<_> = contact_emails
                .iter()
                .enumerate()
                .filter(|(_email_id, email)| email.contact_id == contact.remote_id)
                .map(|(email_id, email)| ContactEmail {
                    id: Some((email_id as u64) + 1),
                    remote_id: email.remote_id.clone(),
                    name: email.name.clone(),
                    defaults: email.defaults,
                    display_order: email.display_order,
                    contact_id: contact.remote_id.clone(),
                    canonical_email: email.canonical_email.clone(),
                    last_used_time: email.last_used_time,
                    is_proton: email.is_proton,
                    label_ids: email.label_ids.clone(),
                    email: email.email.clone(),
                    contact_type: vec![],
                    row_id: None,
                    stash: None,
                })
                .collect();
            contact.contact_emails = contact_emails;
            contact
        })
        .collect()
}

fn create_test_full_contact() -> Contact {
    let mut contact = create_test_partial_contacts().into_iter().next().unwrap();
    let emails = create_test_contact_emails()
        .into_iter()
        .filter(|mail| mail.contact_id == contact.remote_id)
        .collect();
    contact.contact_emails = emails;
    contact.cards = vec![
            ContactCard {
                card_type: CardType::Signed,
                data: CardData::from(r"    BEGIN:VCARD\n    VERSION:4.0\n    FN:ProtonMail Features\n    UID:proton-legacy-139892c2-f691-4118-8c29-061196013e04\n    item1.EMAIL;TYPE=work;PREF=1:features@protonmail.black\n    item2.EMAIL;TYPE=home;PREF=2:features@protonmail.ch\n    END:VCARD".to_owned()), 
                signature: Some(CardSignature::from("-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----"))
            },
            ContactCard {
                card_type: CardType::EncryptedAndSigned,
                data: CardData::from("-----BEGIN PGP MESSAGE-----.*-----END PGP MESSAGE-----"), 
                signature: Some(CardSignature::from("-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----"))
            }
        ];
    contact
}

fn create_test_full_modified_contact() -> (Contact, ContactEmail) {
    let mut contact = create_test_full_contact();
    let removed_mail = contact.contact_emails.pop().unwrap();
    contact.modify_time += 1;
    contact.size += 1;
    contact.contact_emails.push(ContactEmail {
        id: None,
        remote_id: ContactEmailId::from("aefew4323jFv0BhScc=="),
        name: "contact_email_name_mod".to_owned(),
        email: "contact_email_mod@contact.test".to_owned(),
        contact_type: vec![ContactType::from("work")],
        defaults: ContactSendingPreferences::Default,
        display_order: 1,
        contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
        label_ids: vec![ContactLabelId::from(
            "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
        )],
        canonical_email: "contact_email_mod@contact.test".to_owned(),
        last_used_time: 0,
        is_proton: true,
        row_id: None,
        stash: None,
    });
    contact.cards = vec![
        ContactCard {
            card_type: CardType::Signed,
            data: CardData::from(r"    BEGIN:VCARD\n    VERSION:4.0\n    FN:ProtonMail Features\n    UID:proton-legacy-129892c2-f691-4118-8c29-061196013e04\n    item1.EMAIL;TYPE=work;PREF=1:sdfsdf@protonmail.black\n    item2.EMAIL;TYPE=home;PREF=2:features@protonmail.ch\n    END:VCARD".to_owned()), 
            signature: Some(CardSignature::from("-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----"))
        },
        ContactCard {
            card_type: CardType::EncryptedAndSigned,
            data: CardData::from("-----BEGIN PGP MESSAGE-----modified.*-----END PGP MESSAGE-----"), 
            signature: Some(CardSignature::from("-----BEGIN PGP SIGNATURE-----modified.*-----END PGP SIGNATURE-----"))
        }
    ];
    (contact, removed_mail)
}
