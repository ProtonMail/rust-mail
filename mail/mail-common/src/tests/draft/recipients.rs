use super::*;
#[test]
fn duplicate_single_recipient_reports_error() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "foo@example.com".to_owned(),
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
        email: email.clone(),
    };

    list.add_single(entry).unwrap();
    assert_eq!(list.len(), 1);
    list.remove_single(&email);
    assert!(list.is_empty());
}

#[test]
fn invalid_email_is_added_to_list_with_error_status() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "borkenEmail!".to_owned(),
    };

    list.add_single(entry.clone()).unwrap();
    assert_eq!(list.len(), 1);
    match &list.recipients()[0] {
        Recipient::Single(entry) => {
            assert_eq!(entry.state, ValidationState::InvalidEmail);
        }
        _ => panic!("unexpected entry"),
    }
}

#[test]
fn invalid_email_is_added_to_list_with_error_status_group() {
    let mut list = RecipientList::default();
    let entry = RecipientEntry {
        display_name: None,
        email: "borkenEmail!".to_owned(),
    };

    list.add_group(GROUP_NAME, [entry.clone()], 1);
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, GROUP_NAME);
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
        email: "foo@example.com".to_owned(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: "bar@example.com".to_owned(),
    };

    list.add_single(entry.clone()).unwrap();
    let (_, duplicates) = list.add_group(GROUP_NAME, [entry.clone(), entry2], 2);
    assert_eq!(duplicates[0], entry);
    assert_eq!(list.len(), 2);

    match &list.recipients()[1] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, GROUP_NAME);
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
        email: "foo@example.com".to_owned(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: "bar@example.com".to_owned(),
    };

    let (_, duplicates) = list.add_group(GROUP_NAME, [entry.clone()], 1);
    assert!(duplicates.is_empty());
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, GROUP_NAME);
            assert_eq!(entry.total_in_group, 1);
            assert_eq!(entry.recipients.len(), 1);
        }
        _ => panic!("unexpected entry"),
    }

    let (_, duplicates) = list.add_group(GROUP_NAME, [entry2.clone()], 2);
    assert!(duplicates.is_empty());
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, GROUP_NAME);
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
        email: "foo@example.com".to_owned(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: "bar@example.com".to_owned(),
    };

    list.add_group(GROUP_NAME, [entry, entry2], 2);
    assert_eq!(list.len(), 1);
    list.remove_group(GROUP_NAME);
    assert!(list.is_empty());
}

#[test]
fn remove_single_recipient_from_group() {
    let mut list = RecipientList::default();
    let email1 = "foo@example.com".to_owned();
    let email2 = "bar@example.com".to_owned();
    let entry = RecipientEntry {
        display_name: None,
        email: email1.clone(),
    };
    let entry2 = RecipientEntry {
        display_name: None,
        email: email2.clone(),
    };

    list.add_group(GROUP_NAME, [entry, entry2], 2);
    assert_eq!(list.len(), 1);
    list.remove_group_recipient(GROUP_NAME, &email1);
    assert_eq!(list.len(), 1);

    match &list.recipients()[0] {
        Recipient::Group(entry) => {
            assert_eq!(entry.group_name, GROUP_NAME);
            assert_eq!(entry.total_in_group, 2);
            assert_eq!(entry.recipients.len(), 1);
            assert_eq!(entry.recipients[0].email, email2);
        }
        _ => panic!("unexpected entry"),
    }
}

#[test]
fn to_message_recipient_only_copies_valid_values() {
    let mut list = RecipientList::default();

    let valid_entry = RecipientEntry {
        display_name: Some("Foo Ext".to_owned()),
        email: "foo@example.com".to_owned(),
    };

    let valid_proton_entry = RecipientEntry {
        display_name: Some("Foo Proton".to_owned()),
        email: "foo@proton.ch".to_owned(),
    };

    let validating_entry = RecipientEntry {
        display_name: None,
        email: "validating@example.com".to_owned(),
    };

    let unchecked_entry = RecipientEntry {
        display_name: None,
        email: "unchecked@example.com".to_owned(),
    };

    let invalid_email_entry = RecipientEntry {
        display_name: None,
        email: "@".to_owned(),
    };

    let unknown_error_entry = RecipientEntry {
        display_name: None,
        email: "unknown@error.org".to_owned(),
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
            group: None,
        },
        MessageRecipient {
            address: valid_proton_entry.email,
            is_proton: true,
            name: valid_proton_entry.display_name.unwrap_or_default(),
            group: None,
        },
        MessageRecipient {
            address: validating_entry.email,
            is_proton: false,
            name: validating_entry.display_name.unwrap_or_default(),
            group: None,
        },
        MessageRecipient {
            address: unchecked_entry.email,
            is_proton: false,
            name: unchecked_entry.display_name.unwrap_or_default(),
            group: None,
        },
    ];

    assert_eq!(recipients, expected_message_recipients);
}

#[test]
fn to_message_recipient_only_copies_valid_values_group() {
    let mut list = RecipientList::default();

    let valid_entry = RecipientEntry {
        display_name: Some("Foo Ext".to_owned()),
        email: "foo@example.com".to_owned(),
    };

    let valid_proton_entry = RecipientEntry {
        display_name: Some("Foo Proton".to_owned()),
        email: "foo@proton.ch".to_owned(),
    };

    let validating_entry = RecipientEntry {
        display_name: None,
        email: "validating@example.com".to_owned(),
    };

    let unchecked_entry = RecipientEntry {
        display_name: None,
        email: "unchecked@example.com".to_owned(),
    };

    let invalid_email_entry = RecipientEntry {
        display_name: None,
        email: "@".to_owned(),
    };

    let unknown_error_entry = RecipientEntry {
        display_name: None,
        email: "unknown@error.org".to_owned(),
    };

    list.add_group_with_state(
        GROUP_NAME,
        [valid_entry.clone()],
        0,
        ValidationState::Valid(false),
    );
    list.add_group_with_state(
        GROUP_NAME,
        [valid_proton_entry.clone()],
        0,
        ValidationState::Valid(true),
    );
    list.add_group_with_state(
        GROUP_NAME,
        [validating_entry.clone()],
        0,
        ValidationState::Validating,
    );
    // Unchecked is the default state.
    list.add_group(GROUP_NAME, [unchecked_entry.clone()], 0);
    // email validation happen by default
    list.add_group(GROUP_NAME, [invalid_email_entry.clone()], 0);
    list.add_group_with_state(
        GROUP_NAME,
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
            group: Some(GROUP_NAME.to_owned()),
        },
        MessageRecipient {
            address: valid_proton_entry.email,
            is_proton: true,
            name: valid_proton_entry.display_name.unwrap_or_default(),
            group: Some(GROUP_NAME.to_owned()),
        },
        MessageRecipient {
            address: validating_entry.email,
            is_proton: false,
            name: validating_entry.display_name.unwrap_or_default(),
            group: Some(GROUP_NAME.to_owned()),
        },
        MessageRecipient {
            address: unchecked_entry.email,
            is_proton: false,
            name: unchecked_entry.display_name.unwrap_or_default(),
            group: Some(GROUP_NAME.to_owned()),
        },
    ];

    assert_eq!(recipients, expected_message_recipients);
}

const GROUP_NAME: &str = "my_group";
