use proton_core_api::services::proton::{
    ContactEmailId, ContactFull as ApiContactFull, ContactId, ContactUID, LabelId,
};
use proton_core_common::datatypes::{ContactSendingPreferences, ContactTypes, Labels};
use proton_core_common::event_loop::subscriber::CoreEventSubscriber;
use proton_core_common::events::{Action, ContactEmailEvent, CoreEvent};
use proton_core_common::models::{Contact, ContactEmail, ModelIdExtension};
use proton_core_common::test_utils::test_context::TestContext;
use proton_event_loop::subscriber::Subscriber;
use stash::orm::Model;
use std::sync::Arc;

#[tokio::test]
async fn test_contact_email_events_fetch_missing_contact_dependencies() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let test_event_subscriber = CoreEventSubscriber::from(Arc::downgrade(&user_ctx));

    let missing_contact = ApiContactFull {
        id: ContactId::from("missing_contact_id_123"),
        cards: vec![],
        contact_emails: vec![],
        create_time: 1_503_815_366,
        label_ids: vec![LabelId::from("test_label_id")],
        modify_time: 1_503_815_366,
        name: "Missing Contact".to_owned(),
        size: 1443,
        uid: ContactUID::from("proton-legacy-missing-contact-uid"),
    };

    let contact_email_1 = ContactEmail {
        local_id: None,
        remote_id: Some(ContactEmailId::from("email_1_id")),
        local_contact_id: None,
        remote_contact_id: Some(missing_contact.id.clone()),
        canonical_email: "missing.contact.1@example.com".into(),
        contact_type: ContactTypes::default(),
        defaults: ContactSendingPreferences::Default,
        display_order: 1,
        email: "missing.contact.1@example.com".into(),
        is_proton: false,
        label_ids: Labels::default(),
        last_used_time: 0.into(),
        name: "Missing Contact Email 1".to_owned(),
    };

    let contact_email_2 = ContactEmail {
        local_id: None,
        remote_id: Some(ContactEmailId::from("email_2_id")),
        local_contact_id: None,
        remote_contact_id: Some(missing_contact.id.clone()),
        canonical_email: "missing.contact.2@example.com".into(),
        contact_type: ContactTypes::default(),
        defaults: ContactSendingPreferences::Default,
        display_order: 1,
        email: "missing.contact.2@example.com".into(),
        is_proton: false,
        label_ids: Labels::default(),
        last_used_time: 0.into(),
        name: "Missing Contact Email 2".to_owned(),
    };

    ctx.mock_get_full_contact(missing_contact.clone()).await;
    ctx.catch_all().await;

    let contact_email_events = vec![
        ContactEmailEvent {
            remote_id: contact_email_1.remote_id.clone().unwrap(),
            action: Action::Create,
            contact_email: Some(contact_email_1),
        },
        ContactEmailEvent {
            remote_id: contact_email_2.remote_id.clone().unwrap(),
            action: Action::Create,
            contact_email: Some(contact_email_2),
        },
    ];

    let events = CoreEvent {
        contact_emails: Some(contact_email_events),
        ..Default::default()
    };

    test_event_subscriber
        .on_events(&mut [events])
        .await
        .expect("Failed to process events");

    let tether = user_ctx.stash().connection().await.unwrap();
    let stored_contact = Contact::remote_id_counterpart(missing_contact.id, &tether)
        .await
        .unwrap();

    assert!(
        stored_contact.is_some(),
        "Missing contact should have been fetched and stored"
    );
}

#[tokio::test]
async fn test_contact_email_events_ignore_existing_contacts() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let test_event_subscriber = CoreEventSubscriber::from(Arc::downgrade(&user_ctx));

    let existing_contact = Contact {
        local_id: None,
        remote_id: Some(ContactId::from("existing_contact_id")),
        cards: vec![],
        contact_emails: vec![],
        create_time: 1_503_815_366,
        label_ids: Labels::default(),
        modify_time: 1_503_815_366,
        name: "Existing Contact".to_owned(),
        size: 1443,
        uid: ContactUID::from("proton-legacy-existing-contact-uid"),
        deleted: false,
    };

    let mut tether = user_ctx.stash().connection().await.unwrap();
    tether
        .tx(async |tx| existing_contact.clone().save(tx).await)
        .await
        .expect("Failed to store existing contact");

    let contact_email = ContactEmail {
        local_id: None,
        remote_id: Some(ContactEmailId::from("email_existing_id")),
        local_contact_id: None,
        remote_contact_id: existing_contact.remote_id.clone(),
        canonical_email: "existing.contact@example.com".into(),
        contact_type: ContactTypes::default(),
        defaults: ContactSendingPreferences::Default,
        display_order: 1,
        email: "existing.contact@example.com".into(),
        is_proton: false,
        label_ids: Labels::default(),
        last_used_time: 0.into(),
        name: "Existing Contact Email".to_owned(),
    };

    ctx.catch_all().await;

    let contact_email_event = ContactEmailEvent {
        remote_id: contact_email.remote_id.clone().unwrap(),
        action: Action::Create,
        contact_email: Some(contact_email),
    };

    let events = CoreEvent {
        contact_emails: Some(vec![contact_email_event]),
        ..Default::default()
    };

    test_event_subscriber
        .on_events(&mut [events])
        .await
        .expect("Failed to process events");

    let tether = user_ctx.stash().connection().await.unwrap();
    let stored_contact =
        Contact::remote_id_counterpart(existing_contact.remote_id.unwrap(), &tether)
            .await
            .unwrap();

    assert!(
        stored_contact.is_some(),
        "Existing contact should still be present"
    );
}
