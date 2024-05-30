use common::TestContext;
use proton_api_core::domain::{
    CardData, CardSignature, CardType, Contact, ContactCard, ContactEmail, ContactEmailId,
    ContactId, ContactLabelId, ContactPartial, ContactSendingPreferences,
};
use proton_core_common::db::{
    CoreSqliteConnection, LocalContact, LocalContactEmail, LocalContactEmailId, LocalContactId,
    LocalContactWithCards,
};

mod common;

fn create_test_partial_contacts() -> Vec<ContactPartial> {
    vec![
        ContactPartial {
            id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            name: "contact_name".to_owned(),
            uid: "proton-legacy-139892c2-f691-4118-8c29-061196013e04".to_owned(),
            size: 1443,
            create_time: 1_503_815_366,
            modify_time: 1_503_815_366,
            label_ids: vec![ContactLabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
        },
        ContactPartial {
            id: ContactId::from("z29olIjFv0rnXxBhSMz=="),
            name: "contact_name2".to_owned(),
            uid: "proton-legacy-139892c2-f691-4118-8c29-061196013e01".to_owned(),
            size: 1445,
            create_time: 1_503_815_367,
            modify_time: 1_503_815_367,
            label_ids: vec![ContactLabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
        },
    ]
}

fn create_test_contact_emails() -> Vec<ContactEmail> {
    vec![
        ContactEmail {
            id: ContactEmailId::from("aefew4323jFv0BhSMw=="),
            name: "contact_email_name_1".to_owned(),
            email: "contact_email_1@contact.test".to_owned(),
            contact_type: vec!["work".to_owned()],
            defaults: ContactSendingPreferences::Default,
            order: 1,
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            label_ids: vec![ContactLabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
            canonical_email: "contact_email_1@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
        },
        ContactEmail {
            id: ContactEmailId::from("aefew4323jFv0BhSMz=="),
            name: "contact_email_name_2".to_owned(),
            email: "contact_email_2@contact.test".to_owned(),
            contact_type: vec!["work".to_owned()],
            defaults: ContactSendingPreferences::Default,
            order: 1,
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            label_ids: vec![ContactLabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
            canonical_email: "contact_email_2@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
        },
        ContactEmail {
            id: ContactEmailId::from("oZfew4323jFv0BhSMz=="),
            name: "contact_email_name_3".to_owned(),
            email: "contact_email_3@contact.test".to_owned(),
            contact_type: vec!["work".to_owned()],
            defaults: ContactSendingPreferences::Custom,
            order: 1,
            contact_id: ContactId::from("z29olIjFv0rnXxBhSMz=="),
            label_ids: vec![ContactLabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
            canonical_email: "contact_email_3@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
        },
    ]
}

fn expected_local_contacts() -> Vec<LocalContact> {
    let contacts = create_test_partial_contacts();
    let contact_emails = create_test_contact_emails();
    contacts
        .into_iter()
        .enumerate()
        .map(|(index, contact)| {
            let local_contact_id = (index + 1) as u64;
            let contact_emails: Vec<_> = contact_emails
                .iter()
                .enumerate()
                .filter(|(_email_id, email)| email.contact_id == contact.id)
                .map(|(email_id, email)| LocalContactEmail {
                    id: LocalContactEmailId::from((email_id + 1) as u64),
                    rid: Some(email.id.clone()),
                    name: email.name.clone(),
                    defaults: email.defaults,
                    order: email.order,
                    contact_id: LocalContactId::from(local_contact_id),
                    remote_contact_id: Some(contact.id.clone()),
                    canonical_email: email.canonical_email.clone(),
                    last_used_time: email.last_used_time,
                    is_proton: email.is_proton,
                    contact_labels: email.label_ids.clone(),
                    email: email.email.clone(),
                })
                .collect();
            LocalContact {
                id: LocalContactId::from(local_contact_id),
                rid: Some(contact.id),
                name: contact.name,
                uid: contact.uid,
                size: contact.size,
                create_time: contact.create_time,
                modify_time: contact.modify_time,
                contact_emails,
            }
        })
        .collect()
}

fn create_test_full_contact() -> Contact {
    let partial_contact = create_test_partial_contacts().into_iter().next().unwrap();
    let emails = create_test_contact_emails()
        .into_iter()
        .filter(|mail| mail.contact_id == partial_contact.id)
        .collect();
    Contact {
        id: partial_contact.id,
        name: partial_contact.name,
        uid: partial_contact.uid,
        size: partial_contact.size,
        create_time: partial_contact.create_time,
        modify_time: partial_contact.modify_time,
        contact_emails: emails,
        label_ids: partial_contact.label_ids,
        cards: vec![
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
        ],

    }
}

fn expected_local_full_contact() -> LocalContactWithCards {
    let full_contact = create_test_full_contact();
    let local_contact = expected_local_contacts().into_iter().next().unwrap();
    let cards = full_contact.cards.into_iter().map(Into::into).collect();
    LocalContactWithCards {
        local_contact,
        cards,
    }
}

#[test]
fn test_sync_and_load_contacts() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();

    let test_contacts = create_test_partial_contacts();
    let test_contacts_email = create_test_contact_emails();

    // Api mock.
    ctx.async_runtime().block_on(async {
        ctx.mock_get_all_contacts_partial_request(test_contacts.clone())
            .await;
        ctx.mock_get_all_contact_emails_request(test_contacts_email.clone())
            .await;
        ctx.catch_all().await;
    });

    // Sync contacts
    ctx.async_runtime()
        .block_on(user_ctx.sync_contacts())
        .expect("failed to sync contacts");

    // Check database
    let mut conn = user_ctx
        .new_db_connection_as::<CoreSqliteConnection>()
        .expect("expect_db");
    let contacts = conn.tx(|tx| tx.query_contacts(0, 100)).expect("Query ok ");
    let expected_contacts = expected_local_contacts();
    assert_eq!(contacts, expected_contacts);
}

#[test]
fn test_sync_and_load_full_contact() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();

    let test_full_contact = create_test_full_contact();

    // Api mock.
    ctx.async_runtime().block_on(async {
        ctx.mock_get_full_contact(test_full_contact.clone()).await;
        ctx.catch_all().await;
    });

    // Sync contacts
    ctx.async_runtime()
        .block_on(user_ctx.sync_contact_with_card(test_full_contact.id))
        .expect("failed to sync contacts");

    // Check database
    let mut conn = user_ctx
        .new_db_connection_as::<CoreSqliteConnection>()
        .expect("expect_db");
    let contact = conn
        .tx(|tx| tx.query_contact_with_cards(LocalContactId::new(1)))
        .expect("Query ok ")
        .expect("contact should be found");
    let expected_contact = expected_local_full_contact();
    assert_eq!(contact, expected_contact);
}

#[test]
fn test_sync_and_load_contacts_mixed() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();

    let test_contacts = create_test_partial_contacts();
    let test_contacts_email = create_test_contact_emails();
    let test_full_contact = create_test_full_contact();

    // Api mock.
    ctx.async_runtime().block_on(async {
        ctx.mock_get_all_contacts_partial_request(test_contacts.clone())
            .await;
        ctx.mock_get_all_contact_emails_request(test_contacts_email.clone())
            .await;
        ctx.mock_get_full_contact(test_full_contact.clone()).await;
        ctx.catch_all().await;
    });

    // Sync contacts
    ctx.async_runtime()
        .block_on(user_ctx.sync_contacts())
        .expect("failed to sync contacts");
    ctx.async_runtime()
        .block_on(user_ctx.sync_contact_with_card(test_full_contact.id))
        .expect("failed to sync contacts");

    // Check database
    let mut conn = user_ctx
        .new_db_connection_as::<CoreSqliteConnection>()
        .expect("expect_db");

    let contact = conn
        .tx(|tx| tx.query_contact_with_cards(LocalContactId::new(1)))
        .expect("Query ok ")
        .expect("contact should be found");
    let expected_contact = expected_local_full_contact();
    assert_eq!(contact, expected_contact);

    let contacts = conn.tx(|tx| tx.query_contacts(0, 100)).expect("Query ok ");
    let expected_contacts = expected_local_contacts();
    assert_eq!(contacts, expected_contacts);

    let email_to_query = "contact_email_1@contact.test";
    let queried_contacts = conn
        .tx(|tx| tx.query_contact_emails_by_mail(email_to_query))
        .expect("Query ok ");
    let expected_mail = contact
        .local_contact
        .contact_emails
        .iter()
        .find(|email| email.canonical_email == email_to_query)
        .expect("expect to be found");
    assert_eq!(queried_contacts.first().unwrap(), expected_mail);
}
