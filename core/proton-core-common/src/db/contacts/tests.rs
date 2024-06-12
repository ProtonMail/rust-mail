use crate::db::{
    contacts::{LocalContactEmailId, LocalContactId},
    new_core_test_connection, CoreSqliteConnection, CoreSqliteConnectionMut, DBResult,
};
use proton_api_core::domain::{
    CardData, CardSignature, CardType, Contact, ContactCard, ContactEmail, ContactEmailId,
    ContactId, ContactLabelId, ContactPartial, ContactSendingPreferences, ContactType, ContactUid,
};

pub(crate) fn with_tx(conn: &mut CoreSqliteConnection, f: impl Fn(&mut CoreSqliteConnectionMut)) {
    conn.tx(move |tx| -> DBResult<()> {
        (f)(tx);
        Ok(())
    })
    .expect("failed transaction");
}

#[test]
fn test_full_contact() {
    let mut conn = new_core_test_connection();
    with_tx(&mut conn, |tx| {
        let full_contact = create_test_full_contact();
        let id = tx
            .create_or_update_contact(&full_contact)
            .expect("failed to create contact");
        let id_second: LocalContactId = tx
            .create_or_update_contact(&full_contact)
            .expect("failed to overwrite contact");
        assert_eq!(id, LocalContactId::new(1));
        assert_eq!(id, id_second);
        // Query the full contact with cards
        let contact_with_cards = tx
            .query_contact_with_cards(id)
            .expect("query contact with cards failed")
            .expect("expected to find contact");
        assert_eq!(contact_with_cards.cards.len(), full_contact.cards.len());
    });
}

#[test]
fn test_partial_contact() {
    let mut conn = new_core_test_connection();
    with_tx(&mut conn, |tx| {
        let partial_contacts = create_test_partial_contacts();
        let contact_emails = create_test_contact_emails();
        // Insert all partial contacts
        let contact_ids = tx
            .create_or_update_partial_contacts(partial_contacts.iter())
            .expect("failed to create contact");
        // Insert all contact mails
        let mail_ids = tx
            .create_or_update_contact_emails(contact_emails.iter())
            .expect("failed to create contact");
        assert_eq!(contact_ids.first().unwrap(), &LocalContactId::new(1));
        assert_eq!(mail_ids.first().unwrap(), &LocalContactEmailId::new(1));

        // Query specific contact mail.
        let mails = tx
            .query_contact_emails_by_mail("contact_email_1@contact.test")
            .expect("failed to query email");
        assert_eq!(
            mails.first().unwrap().canonical_email,
            "contact_email_1@contact.test"
        );

        // Query all test contact mails.
        let mails = tx
            .query_contact_emails(0, 100)
            .expect("failed to query email");
        assert_eq!(mails.len(), contact_emails.len());

        // Query all contacts.
        let contacts = tx.query_contacts(0, 100).expect("failed to query contacts");
        let contact = contacts.first().unwrap();
        assert_eq!(
            contact.rid.as_ref().unwrap(),
            &ContactId::from("a29olIjFv0rnXxBhSMw==")
        );
        assert_eq!(contact.contact_emails.len(), contact_emails.len());

        // Query specific contact.
        let contact_single = tx
            .query_contact(contact.id)
            .expect("failed to query contacts");
        assert_eq!(contact_single.as_ref().unwrap(), contact);
    });
}

fn create_test_full_contact() -> Contact {
    Contact {
        id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
        name: "contact_name".to_owned(),
        uid: ContactUid::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04"),
        size: 1443,
        create_time: 1_503_815_366,
        modify_time: 1_503_815_366,
        contact_emails: create_test_contact_emails(),
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

    }
}

fn create_test_contact_emails() -> Vec<ContactEmail> {
    vec![
        ContactEmail {
            id: ContactEmailId::from("aefew4323jFv0BhSMw=="),
            name: "contact_email_name_1".to_owned(),
            email: "contact_email_1@contact.test".to_owned(),
            contact_type: vec![ContactType::from("work")],
            defaults: ContactSendingPreferences::Default,
            order: 1,
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            canonical_email: "contact_email_1@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
        },
        ContactEmail {
            id: ContactEmailId::from("aefew4323jFv0BhSMz=="),
            name: "contact_email_name_2".to_owned(),
            email: "contact_email_2@contact.test".to_owned(),
            contact_type: vec![ContactType::from("work")],
            defaults: ContactSendingPreferences::Default,
            order: 1,
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            canonical_email: "contact_email_2@contact.test".to_owned(),
            last_used_time: 0,
            is_proton: true,
        },
    ]
}

fn create_test_partial_contacts() -> Vec<ContactPartial> {
    vec![ContactPartial {
        id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
        name: "contact_name".to_owned(),
        uid: ContactUid::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04".to_owned()),
        size: 1443,
        create_time: 1_503_815_366,
        modify_time: 1_503_815_366,
        label_ids: vec![ContactLabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
    }]
}
