use super::*;
use proton_core_api::services::proton::{ContactEmailId, ContactId, LabelId};
use proton_core_common::datatypes::{LabelType, Labels};
use proton_core_common::models::{Contact, ContactEmail, Label};
use proton_mail_common::test_utils::db::new_test_connection_file;
use stash::orm::Model;
use stash::stash::StashError;

#[test]
fn duplicate_single_recipient_reports_error() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "foo@example.com".into(),
    };

    list.add_single(entry.clone()).unwrap();
    let err = list.add_single(entry).unwrap_err();
    assert!(matches!(err, RecipientError::DuplicateAddress(_)));
}

#[test]
fn remove_single_recipient() {
    let mut list = RecipientList::default();
    let email = "foo@example.com".to_owned();
    let entry = RecipientEntry {
        display_name: None,
        email: email.clone().into(),
    };

    list.add_single(entry).unwrap();
    assert_eq!(list.len(), 1);
    list.remove_single(&email);
    assert!(list.is_empty());
}

#[test]
fn invalid_email_is_added_to_list_with_error_status() {
    let invalid_emails = ["brokenEmail!", "icalid@prprp"];

    for invalid_email in invalid_emails {
        let entry = RecipientEntry {
            display_name: None,
            email: invalid_email.into(),
        };

        let mut list = RecipientList::default();
        list.add_single(entry.clone()).unwrap();
        assert_eq!(list.len(), 1);
        match &list.recipients()[0] {
            Recipient::Single(entry) => {
                assert_eq!(
                    entry.state,
                    ValidationState::InvalidEmail,
                    "Unexpected validation state for {invalid_email}"
                );
            }
            _ => panic!("unexpected entry"),
        }
    }
}

#[test]
fn invalid_email_is_added_to_list_with_error_status_group() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "borkenEmail!".into(),
    };

    list.add_group(group_name_always(), [entry.clone()], 1);
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, group_name_always());
            assert_eq!(entry.total_in_group, 1);
            assert_eq!(entry.recipients.len(), 1);
            assert_eq!(entry.recipients[0].state, ValidationState::InvalidEmail);
        }
        _ => panic!("unexpected entry"),
    }
}

#[test]
fn duplicate_group_recipient_are_returned() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "foo@example.com".into(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: "bar@example.com".into(),
    };

    list.add_single(entry.clone()).unwrap();
    let (_, duplicates) = list.add_group(group_name_always(), [entry.clone(), entry2], 2);
    assert_eq!(duplicates[0], entry);
    assert_eq!(list.len(), 2);

    match &list.recipients()[1] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, group_name_always());
            assert_eq!(entry.total_in_group, 2);
            assert_eq!(entry.recipients.len(), 1);
        }
        _ => panic!("unexpected entry"),
    }
}

#[test]
fn group_extend() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "foo@example.com".into(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: "bar@example.com".into(),
    };

    let (_, duplicates) = list.add_group(group_name_always(), [entry.clone()], 1);
    assert!(duplicates.is_empty());
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, group_name_always());
            assert_eq!(entry.total_in_group, 1);
            assert_eq!(entry.recipients.len(), 1);
        }
        _ => panic!("unexpected entry"),
    }

    let (_, duplicates) = list.add_group(group_name_always(), [entry2.clone()], 2);
    assert!(duplicates.is_empty());
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, group_name_always());
            assert_eq!(entry.total_in_group, 2);
            assert_eq!(entry.recipients.len(), 2);
        }
        _ => panic!("unexpected entry"),
    }
}

#[test]
fn remove_group_recipient() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "foo@example.com".into(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: "bar@example.com".into(),
    };

    list.add_group(group_name_always(), [entry, entry2], 2);
    assert_eq!(list.len(), 1);
    list.remove_group(&group_name_always());
    assert!(list.is_empty());
}

#[test]
fn remove_single_recipient_from_group() {
    let mut list = RecipientList::default();
    let email1 = "foo@example.com".to_owned();
    let email2 = "bar@example.com".to_owned();
    let entry = RecipientEntry {
        display_name: None,
        email: email1.clone().into(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: email2.clone().into(),
    };

    list.add_group(group_name_always(), [entry, entry2], 2);
    assert_eq!(list.len(), 1);
    list.remove_group_recipient(&group_name_always(), &email1);
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, group_name_always());
            assert_eq!(entry.total_in_group, 2);
            assert_eq!(entry.recipients.len(), 1);
            assert_eq!(entry.recipients[0].email.as_clear_text_str(), email2);
        }
        _ => panic!("unexpected entry"),
    }
}

#[test]
fn to_message_recipient_only_copies_valid_values() {
    let mut list = RecipientList::default();

    let valid_entry = RecipientEntry {
        display_name: Some("Foo Ext".into()),
        email: "foo@example.com".into(),
    };

    let valid_proton_entry = RecipientEntry {
        display_name: Some("Foo Proton".into()),
        email: "foo@proton.ch".into(),
    };

    let validating_entry = RecipientEntry {
        display_name: None,
        email: "validating@example.com".into(),
    };

    let unchecked_entry = RecipientEntry {
        display_name: None,
        email: "unchecked@example.com".into(),
    };

    let invalid_email_entry = RecipientEntry {
        display_name: None,
        email: "@".into(),
    };

    let unknown_error_entry = RecipientEntry {
        display_name: None,
        email: "unknown@error.org".into(),
    };

    list.add_single_with_state(valid_entry.clone(), ValidationState::Valid(false))
        .unwrap();
    list.add_single_with_state(valid_proton_entry.clone(), ValidationState::Valid(true))
        .unwrap();
    list.add_single_with_state(validating_entry.clone(), ValidationState::Validating)
        .unwrap();
    // Unchecked is the default state.
    list.add_single(unchecked_entry.clone()).unwrap();
    // email validation happen by default
    list.add_single(invalid_email_entry.clone()).unwrap();
    list.add_single_with_state(unknown_error_entry.clone(), ValidationState::Unknown)
        .unwrap();

    let recipients = list.to_message_recipients();
    let expected_message_recipients = vec![
        MessageRecipient {
            address: valid_entry.email,
            is_proton: false,
            name: valid_entry.display_name.unwrap_or_default(),
            group: MaybeEmptyString(None),
        },
        MessageRecipient {
            address: valid_proton_entry.email,
            is_proton: true,
            name: valid_proton_entry.display_name.unwrap_or_default(),
            group: MaybeEmptyString(None),
        },
        MessageRecipient {
            address: validating_entry.email,
            is_proton: false,
            name: validating_entry.display_name.unwrap_or_default(),
            group: MaybeEmptyString(None),
        },
        MessageRecipient {
            address: unchecked_entry.email,
            is_proton: false,
            name: unchecked_entry.display_name.unwrap_or_default(),
            group: MaybeEmptyString(None),
        },
    ];

    assert_eq!(recipients, expected_message_recipients);
}

#[test]
fn to_message_recipient_only_copies_valid_values_group() {
    let mut list = RecipientList::default();

    let valid_entry = RecipientEntry {
        display_name: Some("Foo Ext".into()),
        email: "foo@example.com".into(),
    };

    let valid_proton_entry = RecipientEntry {
        display_name: Some("Foo Proton".into()),
        email: "foo@proton.ch".into(),
    };

    let validating_entry = RecipientEntry {
        display_name: None,
        email: "validating@example.com".into(),
    };

    let unchecked_entry = RecipientEntry {
        display_name: None,
        email: "unchecked@example.com".into(),
    };

    let invalid_email_entry = RecipientEntry {
        display_name: None,
        email: "@".to_owned().into(),
    };

    let unknown_error_entry = RecipientEntry {
        display_name: None,
        email: "unknown@error.org".into(),
    };

    list.add_group_with_state(
        group_name_always(),
        [valid_entry.clone()],
        0,
        ValidationState::Valid(false),
    );
    list.add_group_with_state(
        group_name_always(),
        [valid_proton_entry.clone()],
        0,
        ValidationState::Valid(true),
    );
    list.add_group_with_state(
        group_name_always(),
        [validating_entry.clone()],
        0,
        ValidationState::Validating,
    );
    // Unchecked is the default state.
    list.add_group(group_name_always(), [unchecked_entry.clone()], 0);
    // email validation happen by default
    list.add_group(group_name_always(), [invalid_email_entry.clone()], 0);
    list.add_group_with_state(
        group_name_always(),
        [unknown_error_entry.clone()],
        0,
        ValidationState::Unknown,
    );

    let recipients = list.to_message_recipients();
    let expected_message_recipients = vec![
        MessageRecipient {
            address: valid_entry.email,
            is_proton: false,
            name: valid_entry.display_name.unwrap_or_default(),
            group: group_name_maybe(),
        },
        MessageRecipient {
            address: valid_proton_entry.email,
            is_proton: true,
            name: valid_proton_entry.display_name.unwrap_or_default(),
            group: group_name_maybe(),
        },
        MessageRecipient {
            address: validating_entry.email,
            is_proton: false,
            name: validating_entry.display_name.unwrap_or_default(),
            group: group_name_maybe(),
        },
        MessageRecipient {
            address: unchecked_entry.email,
            is_proton: false,
            name: unchecked_entry.display_name.unwrap_or_default(),
            group: group_name_maybe(),
        },
    ];

    assert_eq!(recipients, expected_message_recipients);
}

#[tokio::test]
async fn contact_group_resolution_from_message_recipients() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    let contact_group_name = "contact_group".to_owned();
    let unknown_contact_group_name = "unknown".to_owned();

    let contact_group_id = LabelId::from("l2");
    let mut contact_group = Label {
        remote_id: Some(contact_group_id.clone()),
        name: contact_group_name.clone(),
        label_type: LabelType::ContactGroup,
        ..Label::test_default()
    };

    let mut contact1 = Contact {
        remote_id: Some(ContactId::from("123")),
        name: "Barbara Fox".to_string(),
        ..Contact::test_default()
    };
    let mut contact2 = Contact {
        remote_id: Some(ContactId::from("456")),
        name: "Stevie Wonder".to_string(),
        ..Contact::test_default()
    };
    let mut contact1_email = ContactEmail {
        remote_id: Some(ContactEmailId::from("ceid1")),
        label_ids: Labels::new(vec![contact_group_id.clone()]),
        remote_contact_id: contact1.remote_id.clone(),
        ..ContactEmail::test_default()
    };
    let mut contact2_email = ContactEmail {
        remote_id: Some(ContactEmailId::from("ceid2")),
        label_ids: Labels::new(vec![contact_group_id.clone()]),
        remote_contact_id: contact1.remote_id.clone(),
        ..ContactEmail::test_default()
    };
    tether
        .tx::<_, _, StashError>(async |tx| {
            contact_group.save(tx).await.unwrap();
            contact1.save(tx).await.unwrap();
            contact2.save(tx).await.unwrap();
            contact1_email.save(tx).await.unwrap();
            contact2_email.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Note: it doesn't matter if the emails add up, what we are testing is that
    // the total numer of contact in that group is reported correctly.
    let message_recipients = vec![
        MessageRecipient {
            address: "foo@proton.ch".into(),
            is_proton: false,
            name: "".into(),
            group: MaybeEmptyString::from(contact_group_name.clone()),
        },
        MessageRecipient {
            address: "bar@proton.ch".into(),
            is_proton: false,
            name: "".into(),
            group: MaybeEmptyString::from(unknown_contact_group_name.clone()),
        },
        MessageRecipient {
            address: "zzz@proton.ch".into(),
            is_proton: false,
            name: "".into(),
            group: MaybeEmptyString::from(unknown_contact_group_name.clone()),
        },
    ];

    // Create the list.
    let resolver = ProtonContactGroupResolver::new(&tether);
    let mut recipients =
        RecipientList::from_message_recipients(&resolver, message_recipients).await;

    let contact_group_name = NonEmptyString::new(contact_group_name).unwrap();
    let unknown_contact_group_name = NonEmptyString::new(unknown_contact_group_name).unwrap();

    // We only have one contact with this group, but there are 2 members total in the group.
    let group = recipients.find_group_mut(&contact_group_name).unwrap();
    assert_eq!(group.recipients.len(), 1);
    assert_eq!(group.total_in_group, 2);

    // We don't know this group (e.g.: may have been deleted) so the total matches
    // the number of recipients with this group.
    let group = recipients
        .find_group_mut(&unknown_contact_group_name)
        .unwrap();
    assert_eq!(group.recipients.len(), 2);
    assert_eq!(group.total_in_group, 2);
}

#[test]
fn recipient_expiration_feature() {
    let mut list = RecipientList::default();

    let valid_entry = RecipientEntry {
        display_name: Some("Foo Ext".into()),
        email: "foo@example.com".into(),
    };

    let valid_proton_entry = RecipientEntry {
        display_name: Some("Foo Proton".into()),
        email: "foo@proton.ch".into(),
    };

    let validating_entry = RecipientEntry {
        display_name: None,
        email: "validating@example.com".into(),
    };

    let unchecked_entry = RecipientEntry {
        display_name: None,
        email: "unchecked@example.com".into(),
    };

    let invalid_email_entry = RecipientEntry {
        display_name: None,
        email: "@".into(),
    };

    let unknown_error_entry = RecipientEntry {
        display_name: None,
        email: "unknown@error.org".into(),
    };

    let valid_proton_entry_with_is_proton_false = RecipientEntry {
        display_name: None,
        email: "v@pm.me".into(),
    };

    list.add_single_with_state(valid_entry.clone(), ValidationState::Valid(false))
        .unwrap();
    list.add_single_with_state(valid_proton_entry.clone(), ValidationState::Valid(true))
        .unwrap();
    list.add_single_with_state(validating_entry.clone(), ValidationState::Validating)
        .unwrap();
    // Unchecked is the default state.
    list.add_single(unchecked_entry.clone()).unwrap();
    // email validation happen by default
    list.add_single(invalid_email_entry.clone()).unwrap();
    list.add_single_with_state(unknown_error_entry.clone(), ValidationState::Unknown)
        .unwrap();
    list.add_single_with_state(
        valid_proton_entry_with_is_proton_false.clone(),
        ValidationState::Valid(false),
    )
    .unwrap();

    for domain in PROTON_EMAIL_DOMAINS {
        list.add_single(RecipientEntry {
            display_name: None,
            email: format!("bar{domain}").into(),
        })
        .unwrap();
    }

    let mut report = ExpirationFeatureSupportReport::default();

    list.validate_expiration_feature(&mut report);
    assert_eq!(report.supported.len(), PROTON_EMAIL_DOMAINS.len() + 2);
    assert_eq!(report.unknown.len(), 4);
    assert_eq!(report.unsupported.len(), 1);
    assert!(report.unsupported.contains(&valid_entry.email));
    assert!(report.unknown.contains(&validating_entry.email));
    assert!(report.unknown.contains(&unchecked_entry.email));
    assert!(report.unknown.contains(&invalid_email_entry.email));
    assert!(report.unknown.contains(&unknown_error_entry.email));
    assert!(report.supported.contains(&valid_proton_entry.email));
    assert!(
        report
            .supported
            .contains(&valid_proton_entry_with_is_proton_false.email)
    );

    for domain in PROTON_EMAIL_DOMAINS {
        let email: PrivateEmail = format!("bar{domain}").into();
        assert!(!report.unknown.contains(&email));
        assert!(!report.unsupported.contains(&email));
        assert!(report.supported.contains(&email));
    }
}

fn group_name_always() -> NonEmptyString {
    "my_group".to_owned().try_into().unwrap()
}

fn group_name_maybe() -> MaybeEmptyString {
    "my_group".to_owned().into()
}
