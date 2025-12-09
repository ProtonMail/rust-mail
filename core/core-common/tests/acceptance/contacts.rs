#![allow(clippy::needless_pass_by_value)]
use pretty_assertions::assert_eq;
use proton_core_api::services::proton::{
    ContactBasic as ApiContactBasic, ContactCard as ApiContactCard,
    ContactEmail as ApiContactEmail, ContactFull as ApiContactFull,
    ContactSendingPreferences as ApiContactSendingPreferences,
};
use proton_core_api::services::proton::{ContactEmailId, ContactId, ContactUID, LabelId};
use proton_core_common::UserContext;
use proton_core_common::datatypes::{ContactSendingPreferences, ContactTypes, Labels};
use proton_core_common::event_loop::subscriber::CoreEventSubscriber;
use proton_core_common::events::{Action, ContactEvent, CoreEvent};
use proton_core_common::models::{Contact, ContactCard, ContactEmail, ModelIdExtension};
use proton_core_common::test_utils::account::unlocked_user_key;
use proton_core_common::test_utils::test_context::TestContext;
use proton_crypto_account::contacts::ContactCardType;
use proton_crypto_account::proton_crypto::crypto::AccessKeyInfo;
use proton_crypto_account::proton_crypto::new_pgp_provider;
use proton_event_loop::subscriber::Subscriber;
use stash::orm::Model;
use stash::params;
use std::sync::Arc;

macro_rules! prune_email {
    ($email:expr) => {{
        $email.local_id = None;
        $email.local_contact_id = None;
    }};
}

macro_rules! prune_emails {
    ($emails:expr) => {
        for email in $emails.iter_mut() {
            prune_email!(email);
        }
    };
}

macro_rules! prune_card {
    ($card:expr) => {{
        $card.local_id = None;
        $card.local_contact_id = None;
    }};
}

macro_rules! prune_cards {
    ($cards:expr) => {
        for card in $cards.iter_mut() {
            prune_card!(card);
        }
    };
}

macro_rules! prune_contact {
    ($contact:expr) => {{
        $contact.local_id = None;

        prune_emails!(&mut $contact.contact_emails);
        prune_cards!(&mut $contact.cards);
    }};
}

macro_rules! prune_contacts {
    ($contacts:expr) => {
        for contact in $contacts.iter_mut() {
            prune_contact!(contact);
        }
    };
}

#[tokio::test]
async fn test_sync_and_load_contacts() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let test_contacts = create_test_remote_partial_contacts();
    let test_contacts_email = create_test_remote_contact_emails();

    // Api mock.
    ctx.mock_get_all_contacts_partial_request(test_contacts.clone())
        .await;
    ctx.mock_get_all_contact_emails_request(test_contacts_email.clone())
        .await;

    // Sync contacts
    let mut tether = user_ctx.stash().connection().await.unwrap();
    let contacts = Contact::sync(user_ctx.session())
        .await
        .expect("failed to download contacts");
    tether
        .sync_tx(move |tx| contacts.store(tx))
        .await
        .expect("failed to load contacts in db");

    // Check database
    let conn = user_ctx.stash().connection().await.unwrap();
    let mut contacts = Contact::find("LIMIT 100", vec![], &conn)
        .await
        .expect("Failed to get contacts");
    for contact in &mut contacts {
        contact.cards(&conn).await.expect("Failed to query cards");
        contact.emails(&conn).await.expect("Failed to query emails");
    }
    let expected_contacts = expected_local_contacts();
    prune_contacts!(contacts);
    assert_eq!(contacts, expected_contacts);
}

#[tokio::test]
async fn test_sync_and_load_contacts_mixed() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let test_contacts = create_test_remote_partial_contacts();
    let test_contacts_email = create_test_remote_contact_emails();
    let test_full_contact = create_test_remote_full_contact();
    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts.clone(),
        test_contacts_email.clone(),
        test_full_contact.clone(),
    )
    .await;

    // Check database
    let conn = user_ctx.stash().connection().await.unwrap();

    let remote_id = test_contacts.first().unwrap().id.clone();
    let mut contact = Contact::find_by_remote_id(remote_id, &conn)
        .await
        .expect("Failed to load contact")
        .expect("contact should be found");
    contact.cards(&conn).await.expect("Failed to query cards");
    contact.emails(&conn).await.expect("Failed to query emails");
    prune_contact!(contact);
    let expected_contact = create_test_local_full_contact();

    assert_eq!(contact, expected_contact);

    let mut contacts = Contact::find("LIMIT 100", vec![], &conn)
        .await
        .expect("Failed to load contacts");
    for contact in &mut contacts {
        contact.emails(&conn).await.expect("Failed to query emails");
    }
    prune_contacts!(contacts);
    let expected_contacts = expected_local_contacts();
    assert_eq!(contacts, expected_contacts);

    let email_to_query = "KeYtranSparenCymAiler@gmail.com";
    let queried_contact_emails =
        ContactEmail::find("WHERE email = ?", params![email_to_query], &conn)
            .await
            .expect("Failed to get contact emails");
    let expected_mail = contact
        .contact_emails
        .iter()
        .find(|email| email.email.eq_ignore_ascii_case(email_to_query))
        .expect("expect to be found");
    let mut actual_mail = queried_contact_emails[0].clone();

    prune_email!(actual_mail);
    assert_eq!(&actual_mail, expected_mail);
}

#[tokio::test]
async fn test_sync_and_delete_event_contact() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let test_event_subscriber = CoreEventSubscriber::from(Arc::downgrade(&user_ctx));

    let test_contacts = create_test_remote_partial_contacts();
    let test_contacts_email = create_test_remote_contact_emails();
    let test_full_contact = create_test_remote_full_contact();
    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts.clone(),
        test_contacts_email.clone(),
        test_full_contact.clone(),
    )
    .await;

    let contact_to_remove = test_contacts.last().unwrap();

    let delete_contact_event = ContactEvent {
        remote_id: contact_to_remove.id.clone(),
        action: Action::Delete,
        contact: None,
    };
    let events = CoreEvent {
        contacts: Some(vec![delete_contact_event]),
        ..Default::default()
    };
    // Fire event:
    test_event_subscriber
        .on_events(&mut [events])
        .await
        .expect("failed to execute event");

    // Were the  deletions successful?
    let conn = user_ctx.stash().connection().await.unwrap();

    let contacts = Contact::find("LIMIT 100", vec![], &conn)
        .await
        .expect("Failed to get contacts");
    assert_eq!(contacts.len(), test_contacts.len() - 1);
}

#[tokio::test]
async fn test_sync_and_modify_event_contact() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let test_event_subscriber = CoreEventSubscriber::from(Arc::downgrade(&user_ctx));

    let test_contacts = create_test_remote_partial_contacts();
    let test_contacts_email = create_test_remote_contact_emails();
    let test_full_contact = create_test_remote_full_contact();
    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts.clone(),
        test_contacts_email.clone(),
        test_full_contact.clone(),
    )
    .await;

    let (modified_contact, _, _) = create_test_local_full_modified_contact();

    let remote_id = modified_contact.remote_id.clone().unwrap();
    let modify_contact_event = ContactEvent {
        remote_id: remote_id.clone(),
        action: Action::Update,
        contact: Some(modified_contact.clone()),
    };
    let event = CoreEvent {
        contacts: Some(vec![modify_contact_event]),
        ..Default::default()
    };
    // Fire event:
    test_event_subscriber
        .on_events(&mut [event])
        .await
        .expect("failed to execute event");

    let conn = user_ctx.stash().connection().await.unwrap();

    let mut contact = Contact::find_by_remote_id(remote_id, &conn)
        .await
        .expect("Failed to load contact")
        .expect("contact should be found");
    contact
        .emails(&conn)
        .await
        .expect("Failed to query contact emails");

    assert_eq!(contact.modify_time, modified_contact.modify_time);
    assert_eq!(contact.size, modified_contact.size);
    assert_eq!(
        contact.contact_emails.len(),
        modified_contact.contact_emails.len()
    );
    let expected_cards: Vec<ContactCard> = modified_contact.cards.clone();
    contact.cards(&conn).await.expect("Failed to query cards");
    prune_cards!(contact.cards);
    assert_eq!(contact.cards, expected_cards);
}

#[tokio::test]
async fn test_contact_load_public_address_keys() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let test_contacts = create_test_remote_partial_contacts();
    let test_contacts_email = create_test_remote_contact_emails();
    let test_full_contact = create_test_remote_full_contact();
    let contact_email = test_contacts_email.first().unwrap().email.clone();

    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts,
        test_contacts_email,
        test_full_contact,
    )
    .await;

    // Check public address keys from contacts
    let pgp = new_pgp_provider();
    let unlocked_user_keys = unlocked_user_key(&pgp);
    let mut tether = user_ctx.stash().connection().await.unwrap();

    let keys = user_ctx
        .public_address_keys_from_contacts(
            &pgp,
            &mut tether,
            &unlocked_user_keys,
            contact_email.as_ref(),
        )
        .await
        .expect("there should be no error or key extraction")
        .expect("key must be found");

    assert_eq!(keys.pinned_keys.len(), 2);
    assert!(keys.sign.unwrap());
    assert!(keys.encrypt_to_pinned.unwrap());
    let preferred_fingerprint_1 = keys.pinned_keys.first().unwrap().key_fingerprint();

    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let test_contacts = create_test_remote_partial_contacts();
    let test_contacts_email = create_test_remote_contact_emails();
    let mut test_full_contact = create_test_remote_full_contact();
    test_full_contact.cards.remove(0);
    test_full_contact.cards.push(ApiContactCard {
        card_type: ContactCardType::Signed,
        data: VCARD_SWAPPED_PREF.to_owned(),
        signature: Some(VCARD_SIGNATURE_SWAPPED_PREF.to_owned()),
    });
    let contact_email = test_contacts_email.first().unwrap().email.clone();

    prepare_sync_test_data_contacts(
        &ctx,
        &user_ctx,
        test_contacts,
        test_contacts_email,
        test_full_contact,
    )
    .await;

    // Check public address keys from contacts
    let pgp = new_pgp_provider();
    let unlocked_user_keys = unlocked_user_key(&pgp);

    let preferred_fingerprint_2 = user_ctx
        .public_address_keys_from_contacts(
            &pgp,
            &mut tether,
            &unlocked_user_keys,
            contact_email.as_ref(),
        )
        .await
        .expect("there should be no error or key extraction")
        .expect("key must be found")
        .pinned_keys
        .first()
        .unwrap()
        .key_fingerprint();

    assert_ne!(preferred_fingerprint_1, preferred_fingerprint_2);
}

async fn prepare_sync_test_data_contacts(
    ctx: &TestContext,
    user_ctx: &UserContext,
    test_remote_contacts: Vec<ApiContactBasic>,
    test_remote_contacts_email: Vec<ApiContactEmail>,
    test_remote_full_contact: ApiContactFull,
) {
    let remote_contact_id = test_remote_full_contact.id.clone();
    // Api mock.
    ctx.mock_get_all_contacts_partial_request(test_remote_contacts)
        .await;
    ctx.mock_get_all_contact_emails_request(test_remote_contacts_email)
        .await;
    ctx.mock_get_full_contact(test_remote_full_contact).await;

    // Sync contacts
    let mut tether = user_ctx.stash().connection().await.unwrap();
    let contacts = Contact::sync(user_ctx.session())
        .await
        .expect("failed to download contacts");
    tether
        .sync_tx(move |tx| contacts.store(tx))
        .await
        .expect("failed to load contacts in db");

    let local_id = Contact::remote_id_counterpart(remote_contact_id, &tether)
        .await
        .unwrap()
        .unwrap();
    Contact::force_sync_with_card(local_id, user_ctx.session(), &mut tether)
        .await
        .expect("failed to sync contacts");
}

fn create_test_local_partial_contacts() -> Vec<Contact> {
    vec![
        Contact {
            local_id: None,
            remote_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
            cards: vec![],
            contact_emails: vec![],
            create_time: 1_503_815_366,
            label_ids: Labels::new(vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")]),
            modify_time: 1_503_815_366,
            name: "contact_name".to_owned(),
            size: 1443,
            uid: ContactUID::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04"),
            deleted: false,
        },
        Contact {
            local_id: None,
            remote_id: Some(ContactId::from("z29olIjFv0rnXxBhSMz==")),
            cards: vec![],
            contact_emails: vec![],
            create_time: 1_503_815_367,
            label_ids: Labels::new(vec![LabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )]),
            modify_time: 1_503_815_367,
            name: "contact_name2".to_owned(),
            size: 1445,
            uid: ContactUID::from("proton-legacy-139892c2-f691-4118-8c29-061196013e01"),
            deleted: false,
        },
    ]
}

fn create_test_remote_partial_contacts() -> Vec<ApiContactBasic> {
    vec![
        ApiContactBasic {
            id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            create_time: 1_503_815_366,
            label_ids: vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            modify_time: 1_503_815_366,
            name: "contact_name".to_owned(),
            size: 1443,
            uid: ContactUID::from("proton-legacy-139892c2-f691-4118-8c29-061196013e04"),
        },
        ApiContactBasic {
            id: ContactId::from("z29olIjFv0rnXxBhSMz=="),
            create_time: 1_503_815_367,
            label_ids: vec![LabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )],
            modify_time: 1_503_815_367,
            name: "contact_name2".to_owned(),
            size: 1445,
            uid: ContactUID::from("proton-legacy-139892c2-f691-4118-8c29-061196013e01"),
        },
    ]
}

fn create_test_local_contact_emails() -> Vec<ContactEmail> {
    vec![
        ContactEmail {
            local_id: None,
            remote_id: Some(ContactEmailId::from("aefew4323jFv0BhSMw==")),
            local_contact_id: None,
            remote_contact_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
            canonical_email: "keytransparencymailer@gmail.com".into(),
            contact_type: ContactTypes::new(vec!["work".to_owned()]),
            defaults: ContactSendingPreferences::Default,
            display_order: 1,
            email: "keytransparencymailer@gmail.com".into(),
            is_proton: true,
            label_ids: Labels::new(vec![LabelId::from(
                "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
            )]),
            last_used_time: 0.into(),
            name: "contact_email_name_1".to_owned(),
        },
        ContactEmail {
            local_id: None,
            remote_id: Some(ContactEmailId::from("aefew4323jFv0BhSMz==")),
            local_contact_id: None,
            remote_contact_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==")),
            canonical_email: "contact_email_2@contact.test".into(),
            contact_type: ContactTypes::new(vec!["work".to_owned()]),
            defaults: ContactSendingPreferences::Default,
            display_order: 1,
            email: "contact_email_2@contact.test".into(),
            is_proton: true,
            label_ids: Labels::new(vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")]),
            last_used_time: 0.into(),
            name: "contact_email_name_2".to_owned(),
        },
        ContactEmail {
            local_id: None,
            remote_id: Some(ContactEmailId::from("oZfew4323jFv0BhSMz==")),
            local_contact_id: None,
            remote_contact_id: Some(ContactId::from("z29olIjFv0rnXxBhSMz==")),
            canonical_email: "contact_email_3@contact.test".into(),
            contact_type: ContactTypes::new(vec!["work".to_owned()]),
            defaults: ContactSendingPreferences::Custom,
            display_order: 1,
            email: "contact_email_3@contact.test".into(),
            is_proton: true,
            label_ids: Labels::new(vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")]),
            last_used_time: 0.into(),
            name: "contact_email_name_3".to_owned(),
        },
    ]
}

fn create_test_remote_contact_emails() -> Vec<ApiContactEmail> {
    vec![
        ApiContactEmail {
            id: ContactEmailId::from("aefew4323jFv0BhSMw=="),
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            canonical_email: "keytransparencymailer@gmail.com".into(),
            contact_type: vec!["work".to_owned()],
            defaults: ApiContactSendingPreferences::Default,
            email: "keytransparencymailer@gmail.com".into(),
            is_proton: true,
            label_ids: vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            last_used_time: 0,
            name: "contact_email_name_1".to_owned(),
            order: 1,
        },
        ApiContactEmail {
            id: ContactEmailId::from("aefew4323jFv0BhSMz=="),
            contact_id: ContactId::from("a29olIjFv0rnXxBhSMw=="),
            canonical_email: "contact_email_2@contact.test".into(),
            contact_type: vec!["work".to_owned()],
            defaults: ApiContactSendingPreferences::Default,
            email: "contact_email_2@contact.test".into(),
            is_proton: true,
            label_ids: vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            last_used_time: 0,
            name: "contact_email_name_2".to_owned(),
            order: 1,
        },
        ApiContactEmail {
            id: ContactEmailId::from("oZfew4323jFv0BhSMz=="),
            contact_id: ContactId::from("z29olIjFv0rnXxBhSMz=="),
            canonical_email: "contact_email_3@contact.test".into(),
            contact_type: vec!["work".to_owned()],
            defaults: ApiContactSendingPreferences::Custom,
            email: "contact_email_3@contact.test".into(),
            is_proton: true,
            label_ids: vec![LabelId::from("I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==")],
            last_used_time: 0,
            name: "contact_email_name_3".to_owned(),
            order: 1,
        },
    ]
}

fn expected_local_contacts() -> Vec<Contact> {
    let contacts = create_test_local_partial_contacts();
    let contact_emails = create_test_local_contact_emails();
    contacts
        .into_iter()
        .map(|mut contact| {
            let contact_emails: Vec<_> = contact_emails
                .iter()
                .filter(|email| email.remote_contact_id == contact.remote_id)
                .map(|email| ContactEmail {
                    local_id: email.local_id,
                    remote_id: email.remote_id.clone(),
                    local_contact_id: email.local_contact_id,
                    remote_contact_id: contact.remote_id.clone(),
                    canonical_email: email.canonical_email.clone(),
                    contact_type: email.contact_type.clone(),
                    defaults: email.defaults,
                    display_order: email.display_order,
                    email: email.email.clone(),
                    is_proton: email.is_proton,
                    label_ids: email.label_ids.clone(),
                    last_used_time: email.last_used_time,
                    name: email.name.clone(),
                })
                .collect();
            contact.contact_emails = contact_emails;
            contact
        })
        .collect()
}

pub const VCARD: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN;PREF=1:keytransparencymailer@gmail.com\r\nUID:proton-web-5f3acd27-47b5-aea9-4988-52196fbf9c0e\r\nITEM1.EMAIL;PREF=1:keytransparencymailer@gmail.com\r\nITEM1.KEY;PREF=1:data:application/pgp-keys;base64,xjMEZf15lRYJKwYBBAHaRw8BA\r\n QdArPz06hKiOUYSVs6dbHpKSh63bW5/QyIFqRvJ5wOALJnNMkx1a2FzIEJ1cmtoYWx0ZXIgPGtl\r\n eXRyYW5zcGFyZW5jeW1haWxlckBnbWFpbC5jb20+wo8EExYIADcWIQSNEf53FU6EMmZs43pG8Pp\r\n wjTNiIAUCZf15lQUJBaOagAIbAwQLCQgHBRUICQoLBRYCAwEAAAoJEEbw+nCNM2IgaX0BANKGrE\r\n NgM7nbpt5uORfaT5JLx695q1RgKDetm6bQhB1/AQDHvY3oha+eabN+yKcOWKlvvNpbbbYzjunnr\r\n mfm7d+HDM44BGX9eZUSCisGAQQBl1UBBQEBB0Aq4KRFu4d/XmR2UEGjsXeWCWvvKUkzsCR/wRDn\r\n 8E/lRQMBCAfCfgQYFggAJhYhBI0R/ncVToQyZmzjekbw+nCNM2IgBQJl/XmVBQkFo5qAAhsMAAo\r\n JEEbw+nCNM2IgEzcBAPqEmyOcnbzbsGJaZ5uFEA3OfGH7anEg2xEbfZ0jxAh0AP9nsO+JqQrVW5\r\n m3aGW4MRMFRjnC2DIHthThNQMw1bZpDQ==\r\nITEM1.KEY;PREF=2:data:application/pgp-keys;base64,xsDNBGZqv0ABDAC0hqYS26MWx\r\n 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\r\nITEM1.X-PM-ENCRYPT:true\r\nITEM1.X-PM-SIGN:true\r\nEND:VCARD";
pub const VCARD_SIGNATURE: &str = "-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwrsEABYKAG0FgmeXntEJkDicqBtFkGUZRRQAAAAAABwAIHNhbHRAbm90YXRpb25z\nLm9wZW5wZ3Bqcy5vcmfLxuZq7ph0q9NiYStwhiISk7Av5qoO+IL6q3nLviRVzRYh\nBOZJEArPLqrMMxX8fzicqBtFkGUZAAB5wQD+OdEIqJHdAXmo39FgjgEkpwbRQ2DN\nUpUDKkKADv0DMGMBALvmNfupBlCeR2tfqrdGwwcGhj1aQuUvHTvBvBH6GBkK\n=gWa1\n-----END PGP SIGNATURE-----\n";

// the SWAPPED_PREF versions match the original however the preferences of the two keys have been swapped so the pref=1 key of VCARD is the pref=2 key in VCARD_SWAPPED_PREF (and vice versa)
pub const VCARD_SWAPPED_PREF: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN;PREF=1:keytransparencymailer@gmail.com\r\nUID:proton-web-5f3acd27-47b5-aea9-4988-52196fbf9c0e\r\nITEM1.EMAIL;PREF=1:keytransparencymailer@gmail.com\r\nITEM1.KEY;PREF=2:data:application/pgp-keys;base64,xjMEZf15lRYJKwYBBAHaRw8BA\r\n QdArPz06hKiOUYSVs6dbHpKSh63bW5/QyIFqRvJ5wOALJnNMkx1a2FzIEJ1cmtoYWx0ZXIgPGtl\r\n eXRyYW5zcGFyZW5jeW1haWxlckBnbWFpbC5jb20+wo8EExYIADcWIQSNEf53FU6EMmZs43pG8Pp\r\n wjTNiIAUCZf15lQUJBaOagAIbAwQLCQgHBRUICQoLBRYCAwEAAAoJEEbw+nCNM2IgaX0BANKGrE\r\n NgM7nbpt5uORfaT5JLx695q1RgKDetm6bQhB1/AQDHvY3oha+eabN+yKcOWKlvvNpbbbYzjunnr\r\n mfm7d+HDM44BGX9eZUSCisGAQQBl1UBBQEBB0Aq4KRFu4d/XmR2UEGjsXeWCWvvKUkzsCR/wRDn\r\n 8E/lRQMBCAfCfgQYFggAJhYhBI0R/ncVToQyZmzjekbw+nCNM2IgBQJl/XmVBQkFo5qAAhsMAAo\r\n JEEbw+nCNM2IgEzcBAPqEmyOcnbzbsGJaZ5uFEA3OfGH7anEg2xEbfZ0jxAh0AP9nsO+JqQrVW5\r\n m3aGW4MRMFRjnC2DIHthThNQMw1bZpDQ==\r\nITEM1.KEY;PREF=1:data:application/pgp-keys;base64,xsDNBGZqv0ABDAC0hqYS26MWx\r\n 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\r\nITEM1.X-PM-ENCRYPT:true\r\nITEM1.X-PM-SIGN:true\r\nEND:VCARD";
pub const VCARD_SIGNATURE_SWAPPED_PREF: &str = "-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwrsEABYKAG0FgmeXoL4JkDicqBtFkGUZRRQAAAAAABwAIHNhbHRAbm90YXRpb25z\nLm9wZW5wZ3Bqcy5vcmf2nWW2ygLDAHnABIGRBKjYUe5s8qHb1hOEKvCz2sRJQxYh\nBOZJEArPLqrMMxX8fzicqBtFkGUZAADZZQEApMdTRNdSqBOi1so70/FXGgmPYTo0\nDxmgZwr3ucZcdKcBAK1h6BlaEXC0sh/BNoDiGNjrGN5bXi2mNSp1LDyOxpIK\n=1GO5\n-----END PGP SIGNATURE-----\n";

fn create_test_local_full_contact() -> Contact {
    let partial_contact = create_test_local_partial_contacts()
        .into_iter()
        .next()
        .unwrap();
    let emails = create_test_local_contact_emails()
        .into_iter()
        .filter(|mail| mail.remote_contact_id == partial_contact.remote_id)
        .collect();
    Contact {
        local_id: None,
        remote_id: partial_contact.remote_id.clone(),
        name: partial_contact.name,
        uid: partial_contact.uid,
        size: partial_contact.size,
        create_time: partial_contact.create_time,
        modify_time: partial_contact.modify_time,
        contact_emails: emails,
        label_ids: partial_contact.label_ids,
        deleted: false,
        cards: vec![
            ContactCard {
                local_id: None,
                local_contact_id: None,
                remote_contact_id: partial_contact.remote_id.clone(),
                card_type: ContactCardType::Signed,
                data: VCARD.to_owned(),
                signature: Some(VCARD_SIGNATURE.to_owned()),
            },
            ContactCard {
                local_id: None,
                local_contact_id: None,
                remote_contact_id: partial_contact.remote_id.clone(),
                card_type: ContactCardType::EncryptedAndSigned,
                data: "-----BEGIN PGP MESSAGE-----.*-----END PGP MESSAGE-----".to_owned(),
                signature: Some(
                    "-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----".to_owned(),
                ),
            },
        ],
    }
}

fn create_test_remote_full_contact() -> ApiContactFull {
    let remote_partial_contact = create_test_remote_partial_contacts()
        .into_iter()
        .next()
        .unwrap();
    let emails = create_test_remote_contact_emails()
        .into_iter()
        .filter(|mail| mail.contact_id == remote_partial_contact.id)
        .collect();

    let cards = vec![
        ApiContactCard {
            card_type: ContactCardType::Signed,
            data: VCARD.to_owned(),
            signature: Some(VCARD_SIGNATURE.to_owned()),
        },
        ApiContactCard {
            card_type: ContactCardType::EncryptedAndSigned,
            data: "-----BEGIN PGP MESSAGE-----.*-----END PGP MESSAGE-----".to_owned(),
            signature: Some(
                "-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----".to_owned(),
            ),
        },
    ];

    ApiContactFull {
        id: remote_partial_contact.id,
        cards,
        contact_emails: emails,
        create_time: remote_partial_contact.create_time,
        label_ids: remote_partial_contact.label_ids,
        modify_time: remote_partial_contact.modify_time,
        name: remote_partial_contact.name,
        size: 1443,
        uid: remote_partial_contact.uid,
    }
}

fn create_test_local_full_modified_contact() -> (Contact, ContactEmail, ContactEmail) {
    let mut contact = create_test_local_full_contact();
    let removed_mail = contact.contact_emails.pop().unwrap();
    let new_email = ContactEmail {
        local_id: None,
        remote_id: Some(ContactEmailId::from("aefew4323jFv0BhScc==".to_owned())),
        local_contact_id: None,
        remote_contact_id: Some(ContactId::from("a29olIjFv0rnXxBhSMw==".to_owned())),
        canonical_email: "contact_email_mod@contact.test".into(),
        contact_type: ContactTypes::new(vec!["work".to_owned()]),
        defaults: ContactSendingPreferences::Default,
        display_order: 1,
        email: "contact_email_mod@contact.test".into(),
        is_proton: true,
        label_ids: Labels::new(vec![LabelId::from(
            "I6hgx3Ol-d3HYa3E394T_ACXDmTaBub14w==".to_owned(),
        )]),
        last_used_time: 0.into(),
        name: "contact_email_name_mod".to_owned(),
    };
    contact.modify_time += 1;
    contact.size += 1;
    contact.contact_emails.push(new_email.clone());
    contact.cards = vec![
        ContactCard {
            local_id: None,
            local_contact_id: None,
            remote_contact_id: contact.remote_id.clone(),
            card_type: ContactCardType::Signed,
            data: r"    BEGIN:VCARD\n    VERSION:4.0\n    FN:ProtonMail Features\n    UID:proton-legacy-129892c2-f691-4118-8c29-061196013e04\n    item1.EMAIL;TYPE=work;PREF=1:sdfsdf@protonmail.black\n    item2.EMAIL;TYPE=home;PREF=2:features@protonmail.ch\n    END:VCARD".to_owned(),
            signature: Some("-----BEGIN PGP SIGNATURE-----.*-----END PGP SIGNATURE-----".to_owned()),
        },
        ContactCard {
            local_id: None,
            local_contact_id: None,
            remote_contact_id: contact.remote_id.clone(),
            card_type: ContactCardType::EncryptedAndSigned,
            data: "-----BEGIN PGP MESSAGE-----modified.*-----END PGP MESSAGE-----".to_owned(),
            signature: Some("-----BEGIN PGP SIGNATURE-----modified.*-----END PGP SIGNATURE-----".to_owned()),
        }
    ];
    (contact, removed_mail, new_email)
}
