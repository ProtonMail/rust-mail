use crate::datatypes::{ContactSendingPreferences, ContactTypes, Labels};
use crate::models::{Contact, ContactCard, ContactEmail};
use crate::tests::common::new_core_test_connection;
use proton_core_api::services::proton::{ContactEmailId, ContactId, ContactUID, LabelId};
use proton_crypto_account::contacts::ContactCardType;
use stash::orm::Model;
use stash::params;
use stash::stash::StashError;

#[tokio::test]
async fn test_full_contact() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    let mut full_contact = create_test_full_contact();
    let local_id = tether
        .tx::<_, _, StashError>(async |tx| {
            full_contact
                .save(tx)
                .await
                .expect("failed to create contact");
            let id = full_contact.local_id.expect("failed to get contact id");
            let local_id = full_contact.id();
            full_contact
                .save(tx)
                .await
                .expect("failed to overwrite contact");
            let id_second = full_contact.local_id.expect("failed to get contact id");
            assert_eq!(id, 1.into());
            assert_eq!(id, id_second);

            Ok(local_id)
        })
        .await
        .unwrap();

    // Query the full contact with cards
    let mut contact_with_cards = Contact::load(local_id, &tether)
        .await
        .expect("query contact with cards failed")
        .expect("expected to find contact");
    let cards = contact_with_cards
        .cards(&tether)
        .await
        .expect("Failed to query cards");
    assert_eq!(cards.len(), full_contact.cards.len());
}

#[tokio::test]
async fn test_partial_contact() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    let mut partial_contacts = create_test_partial_contacts();
    let mut contact_emails = create_test_contact_emails();
    tether
        .tx::<_, _, StashError>(async |tx| {
            // Insert all partial contacts
            for contact in &mut partial_contacts {
                contact.save(tx).await.expect("failed to create contact");
            }
            // Insert all contact mails
            for contact_email in &mut contact_emails {
                contact_email.remote_contact_id =
                    partial_contacts.first().unwrap().remote_id.clone();
                contact_email
                    .save(tx)
                    .await
                    .expect("failed to create contact email");
            }
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(
        partial_contacts.first().unwrap().local_id.unwrap(),
        1.into()
    );
    assert_eq!(contact_emails.first().unwrap().local_id.unwrap(), 1.into());

    // Query specific contact mail.
    let mail = ContactEmail::find_first(
        "WHERE canonical_email = ?",
        params!["contact_email_1@contact.test"],
        &tether,
    )
    .await
    .expect("failed to query email")
    .expect("expected to find contact email");
    assert_eq!(
        mail.canonical_email.as_clear_text_str(),
        "contact_email_1@contact.test"
    );

    // Query all test contact mails.
    let mails = ContactEmail::find("LIMIT 100", vec![], &tether)
        .await
        .expect("failed to query email");
    assert_eq!(mails.len(), contact_emails.len());

    // Query all contacts.
    let mut contacts = Contact::find("LIMIT 100", vec![], &tether)
        .await
        .expect("failed to query contacts");
    let contact = contacts.first_mut().unwrap();
    assert_eq!(
        contact.remote_id,
        Some(ContactId::from("a29olIjFv0rnXxBhSMw=="))
    );
    assert_eq!(contact.contact_emails.len(), contact_emails.len());

    // Query specific contact.
    let mut contact_single = Contact::load(contact.id(), &tether)
        .await
        .expect("failed to query contacts")
        .expect("expected to find contact");
    contact_single
        .cards(&tether)
        .await
        .expect("failed to query cards");
    assert_eq!(&contact_single, contact);
}

fn create_test_full_contact() -> Contact {
    Contact {
        local_id: None,
        remote_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
        name: "contact_name".to_owned(),
        uid: ContactUID::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04"),
        size: 1443,
        create_time: 1_503_815_366,
        modify_time: 1_503_815_366,
        contact_emails: create_test_contact_emails(),
        label_ids: Labels::new(vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")]),
        deleted: false,
        cards: vec![
            ContactCard {
                local_id: None,
                local_contact_id: None,
                remote_contact_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
                card_type: ContactCardType::Signed,
                data: r"    BEGIN:VCARD\n    VERSION:4.0\n    FN:ProtonMail Features\n    UID:proton-legacy-139892c2-f691-4118-8c29-061196013e04\n    item1.EMAIL;TYPE=work;PREF=1:features@protonmail.black\n    item2.EMAIL;TYPE=home;PREF=2:features@protonmail.ch\n    END:VCARD".to_owned(),
                signature: Some("-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----".to_owned()),
            },
            ContactCard {
                local_id: None,
                local_contact_id: None,
                remote_contact_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
                card_type: ContactCardType::EncryptedAndSigned,
                data: "-----BEGIN PGP MESSAGE-----.*-----END PGP MESSAGE-----".to_owned(),
                signature: Some("-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----".to_owned()),
            }
        ],
    }
}

fn create_test_contact_emails() -> Vec<ContactEmail> {
    vec![
        ContactEmail {
            local_id: None,
            remote_id: Some(ContactEmailId::from("aefew4323jFv0BhSMw==")),
            name: "contact_email_name_1".to_owned(),
            email: "contact_email_1@contact.test".into(),
            contact_type: ContactTypes::new(vec!["work".to_owned()]),
            defaults: ContactSendingPreferences::Default,
            display_order: 1,
            remote_contact_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
            local_contact_id: None,
            label_ids: Labels::new(vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")]),
            canonical_email: "contact_email_1@contact.test".into(),
            last_used_time: 0.into(),
            is_proton: true,
        },
        ContactEmail {
            local_id: None,
            remote_id: Some(ContactEmailId::from("aefew4323jFv0BhSMz==")),
            name: "contact_email_name_2".to_owned(),
            email: "contact_email_2@contact.test".into(),
            contact_type: ContactTypes::new(vec!["work".to_owned()]),
            defaults: ContactSendingPreferences::Default,
            display_order: 1,
            remote_contact_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
            local_contact_id: None,
            label_ids: Labels::new(vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")]),
            canonical_email: "contact_email_2@contact.test".into(),
            last_used_time: 0.into(),
            is_proton: true,
        },
    ]
}

fn create_test_partial_contacts() -> Vec<Contact> {
    vec![Contact {
        local_id: None,
        remote_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
        name: "contact_name".to_owned(),
        uid: ContactUID::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04".to_owned()),
        size: 1443,
        create_time: 1_503_815_366,
        modify_time: 1_503_815_366,
        contact_emails: vec![],
        label_ids: Labels::new(vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")]),
        cards: vec![],
        deleted: false,
    }]
}
