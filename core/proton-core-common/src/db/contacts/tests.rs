use crate::db::new_core_test_connection;
use proton_api_core::domain::{
    CardData, CardSignature, CardType, Contact, ContactCard, ContactEmail, ContactEmailId,
    ContactId, ContactLabelId, ContactSendingPreferences, ContactType, ContactUid,
};
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;

#[tokio::test]
async fn test_full_contact() {
    let conn = new_core_test_connection().await;
    let tx = conn.transaction().await.expect("Failed to start transaction");
        let mut full_contact = create_test_full_contact(&conn);
        full_contact.save_using(&tx).await
            .expect("failed to create contact");
        let id = full_contact.id.clone()
            .expect("failed to get contact id");
        full_contact.save_using(&tx).await
            .expect("failed to overwrite contact");
        let id_second = full_contact.id.clone()
            .expect("failed to get contact id");
        assert_eq!(id, 1);
        assert_eq!(id, id_second);
        // Query the full contact with cards
        let contact_with_cards = Contact::load_using(id, &tx).await
            .expect("query contact with cards failed")
            .expect("expected to find contact");
        assert_eq!(contact_with_cards.cards.len(), full_contact.cards.len());
    tx.commit().await.expect("Failed to commit transaction");
}

#[tokio::test]
async fn test_partial_contact() {
    let conn = new_core_test_connection().await;
    let tx = conn.transaction().await.expect("Failed to start transaction");
        let mut partial_contacts = create_test_partial_contacts();
        let mut contact_emails = create_test_contact_emails(&conn);
        // Insert all partial contacts
        for contact in &mut partial_contacts {
            contact.save_using(&tx).await
                .expect("failed to create contact");
        }
        // Insert all contact mails
        for contact_email in &mut contact_emails {
            contact_email.save_using(&tx).await
                .expect("failed to create contact email");
        }
    tx.commit().await.expect("Failed to commit transaction");
    
        assert_eq!(partial_contacts.first().unwrap().id.unwrap(), 1);
        assert_eq!(contact_emails.first().unwrap().id.unwrap(), 1);

        // Query specific contact mail.
        let mail = ContactEmail::find_first("WHERE canonical_email = ?", params!["contact_email_1@contact.test"], &conn).await
            .expect("failed to query email")
            .expect("expected to find contact email");
        assert_eq!(
            mail.canonical_email,
            "contact_email_1@contact.test"
        );

        // Query all test contact mails.
        let mails = ContactEmail::find("LIMIT 100", vec![], &conn, None).await
            .expect("failed to query email");
        assert_eq!(mails.len(), contact_emails.len());

        // Query all contacts.
        let contacts = Contact::find("LIMIT 100", vec![], &conn, None).await
            .expect("failed to query contacts");
        let contact = contacts.first().unwrap();
        assert_eq!(
            contact.remote_id,
            ContactId::from("a29olIjFv0rnXxBhSMw==")
        );
        assert_eq!(contact.contact_emails.len(), contact_emails.len());

        // Query specific contact.
        let contact_single = Contact::load(contact.id.unwrap(), &conn).await
            .expect("failed to query contacts")
            .expect("expected to find contact");
        assert_eq!(&contact_single, contact);
}

fn create_test_full_contact(stash: &Stash) -> Contact {
    Contact {
        id: None,
        remote_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
        name: "contact_name".to_owned(),
        uid: ContactUid::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04"),
        size: 1443,
        create_time: 1_503_815_366,
        modify_time: 1_503_815_366,
        contact_emails: create_test_contact_emails(stash),
        label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
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
        row_id: None,
        stash: Some(stash.clone()),
    }
}

fn create_test_contact_emails(stash: &Stash) -> Vec<ContactEmail> {
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
            label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            canonical_email: "contact_email_1@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
            row_id: None,
            stash: Some(stash.clone()),
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
            stash: Some(stash.clone()),
        },
    ]
}

fn create_test_partial_contacts() -> Vec<Contact> {
    vec![Contact {
        id: None,
        remote_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
        name: "contact_name".to_owned(),
        uid: ContactUid::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04".to_owned()),
        size: 1443,
        create_time: 1_503_815_366,
        modify_time: 1_503_815_366,
        contact_emails: vec![],
        label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
        cards: vec![],
        row_id: None,
        stash: None,
    }]
}
