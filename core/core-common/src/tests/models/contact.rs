#![allow(clippy::needless_pass_by_value)]
mod contact_list {
    use crate::datatypes::Labels;
    use crate::{
        ceid, cid, contact, contact_email,
        datatypes::{
            AvatarInformation, ContactEmailItem, ContactGroupItem, ContactItem, ContactItemType,
            GroupedContacts, LabelType,
        },
        label, label_id, labels, lid,
        models::{Contact, ContactEmail, Label},
        tests::common::new_core_test_connection,
    };
    use pretty_assertions::assert_eq;
    use proton_api_core::services::proton::common::LabelId;
    use test_case::test_case;

    #[test_case(vec![], vec![], vec![]; "TEST 0 Empty")]
    #[test_case(vec![contact!(local_id: lid!(123), name: "Barbara Lox".to_string())], vec![],
    vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            items: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "Barbara Lox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#A839A4".to_string(),
                        },
                        emails: vec![],
                    },
                ),
            ],
        }
    ]; "TEST 1 Basic")]
    #[test_case(vec![
        contact!(local_id: lid!(123), name: "Barbara Lox".to_string()),
        contact!(local_id: lid!(123), name: "Barbara Fox".to_string())
    ], vec![], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            items: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "Barbara Fox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#1ED19C".to_string(),
                        },
                        emails: vec![],
                    },
                ),
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "Barbara Lox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#A839A4".to_string(),
                        },
                        emails: vec![],
                    },
                ),
            ],
        }
    ]; "TEST 2 Alphabetical order")]
    #[test_case(vec![
        contact!(local_id: lid!(123), name: "🐂 Barbara Lox".to_string()),
        contact!(local_id: lid!(123), name: "🦊 Barbara Fox".to_string())
    ], vec![], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            items: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "🦊 Barbara Fox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#3C8B8C".to_string(),
                        },
                        emails: vec![],
                    },
                ),
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "🐂 Barbara Lox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#415DF0".to_string(),
                        },
                        emails: vec![],
                    },
                ),
            ],
        }
    ]; "TEST 3 With emojis")]
    #[test_case(vec![
        contact!(local_id: lid!(123), name: "🙀".to_string()),
        contact!(local_id: lid!(123), name: "🙀 Barbara Lox".to_string()),
        contact!(local_id: lid!(123), name: "❤️‍🔥 Barbara Fox".to_string())
    ], vec![],
    vec![
        GroupedContacts {
            grouped_by: "#".to_string(),
            items: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id:  123.into(),
                        name: "🙀".to_string(),
                        avatar_information: AvatarInformation {
                            text: "?".to_string(),
                            color: "#3CBB3A".to_string(),
                        },
                        emails: vec![],
                    },
                ),
            ],
        },
        GroupedContacts {
            grouped_by: "B".to_string(),
            items: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "❤️‍🔥 Barbara Fox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#0047AB".to_string(),
                        },
                        emails: vec![],
                    },
                ),
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "🙀 Barbara Lox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#4989FF".to_string(),
                        },
                        emails: vec![],
                    },
                ),
            ],
        }
    ]; "TEST 4 Mutliple groups")]
    #[test_case(vec![
        contact!(local_id: lid!(123), label_ids: labels!("family"), name: "Mom".to_string()),
        contact!(local_id: lid!(124), label_ids: labels!("family"), name: "Dad".to_string()),
        contact!(local_id: lid!(125), label_ids: labels!("family"), name: "Sister".to_string())
    ], vec![
        label!(local_id: lid!(100), remote_id: Some(label_id!("family")), name: "Family".to_string(), label_type: LabelType::ContactGroup)
    ], vec![
        GroupedContacts {
            grouped_by: "D".to_string(),
            items: vec![
                ContactItemType::Contact (
                    ContactItem {
                        local_id: 124.into(),
                        name: "Dad".to_string(),
                        avatar_information: AvatarInformation {
                            text: "D".to_string(),
                            color: "#A839A4".to_string(),
                        },
                        emails: vec![]
                    }
                ),
            ]
        },
        GroupedContacts {
            grouped_by: "F".to_string(),
            items: vec![
                ContactItemType::Group (
                    ContactGroupItem {
                        local_id: 100.into(),
                        name: "Family".to_string(),
                        avatar_information: AvatarInformation {
                            text: "F".to_string(),
                            color: "#A839A4".to_string(),
                        },
                        contacts: vec![
                            ContactItem {
                                local_id: 124.into(),
                                name: "Dad".to_string(),
                                avatar_information: AvatarInformation {
                                    text: "D".to_string(),
                                    color: "#A839A4".to_string(),
                                },
                                emails: vec![]
                            },
                            ContactItem {
                                local_id: 123.into(),
                                name: "Mom".to_string(),
                                avatar_information: AvatarInformation {
                                    text: "M".to_string(),
                                    color: "#213474".to_string(),
                                },
                                emails: vec![]
                            },
                            ContactItem {
                                local_id: 125.into(),
                                name: "Sister".to_string(),
                                avatar_information: AvatarInformation {
                                    text: "S".to_string(),
                                    color: "#9C89FF".to_string(),
                                },
                                emails: vec![]
                            }
                        ]
                    }
                ),
            ]
        },
        GroupedContacts {
            grouped_by: "M".to_string(),
            items: vec![
                ContactItemType::Contact (
                    ContactItem {
                        local_id: 123.into(),
                        name: "Mom".to_string(),
                        avatar_information: AvatarInformation {
                            text: "M".to_string(),
                            color: "#213474".to_string(),
                        },
                        emails: vec![]
                    }
                ),
            ]
        },
        GroupedContacts {
            grouped_by: "S".to_string(),
            items: vec![
                ContactItemType::Contact (
                    ContactItem {
                        local_id: 125.into(),
                        name: "Sister".to_string(),
                        avatar_information: AvatarInformation {
                            text: "S".to_string(),
                            color: "#9C89FF".to_string(),
                        },
                        emails: vec![]
                    }
                ),
            ]
        },

    ]; "TEST 5 Contact groups (labels)")]
    fn test_grouped_contacts(
        contacts: Vec<Contact>,
        groups: Vec<Label>,
        expected: Vec<GroupedContacts>,
    ) {
        let result = GroupedContacts::from_contacts_and_groups(contacts, groups);
        assert_eq!(result, expected);
    }

    #[test_case(vec![
        contact_email!(remote_id: ceid!("3"), email: "barbara1984@yahoo.com".to_string(), display_order: 3),
        contact_email!(remote_id: ceid!("1"), email: "barbara@fox.us".to_string(), display_order: 2),
        contact_email!(remote_id: ceid!("2"), email: "bfox@proton.me".to_string(), display_order: 1, is_proton: true),
    ], vec![
    GroupedContacts {
        grouped_by: "B".to_string(),
        items: vec![
            ContactItemType::Contact(
                ContactItem {
                    local_id: 1.into(),
                    name: "Barbara Fox".to_string(),
                    avatar_information: AvatarInformation {
                        text: "B".to_string(),
                        color: "#1ED19C".to_string(),
                    },
                    emails: vec![
                        ContactEmailItem {
                            local_id: 3.into(),
                            email: "bfox@proton.me".to_string(),
                            is_proton: true,
                            last_used_time: 0,
                        },
                        ContactEmailItem {
                            local_id: 2.into(),
                            email: "barbara@fox.us".to_string(),
                            is_proton: false,
                            last_used_time: 0,
                        },
                        ContactEmailItem {
                            local_id: 1.into(),
                            email: "barbara1984@yahoo.com".to_string(),
                            is_proton: false,
                            last_used_time: 0,
                        },
                    ],
                },
            ),
        ],
    },
    ]; "TEST 1 emails are sorted by display order")]
    #[tokio::test]
    async fn test_grouped_contacts_emails_order(
        emails: Vec<ContactEmail>,
        expected: Vec<GroupedContacts>,
    ) {
        let mut tether = new_core_test_connection().await.connection();
        let mut contact = contact!(remote_id: cid!("123"), name: "Barbara Fox".to_string());
        let tx = tether.transaction().await.unwrap();
        contact.save(&tx).await.unwrap();

        for mut email in emails {
            email.remote_contact_id = contact.remote_id.clone();
            email.save(&tx).await.unwrap();
        }
        tx.commit().await.expect("commit failed");

        let result = Contact::contact_list(&tether).await.unwrap();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_count_email_group_count() {
        let mut tether = new_core_test_connection().await.connection();

        let empty_group_id = LabelId::from("l1");
        let not_empty_group_id = LabelId::from("l2");
        let mut contact_group_empty = Label {
            remote_id: Some(empty_group_id.clone()),
            name: "contact_group_empty".to_owned(),
            label_type: LabelType::ContactGroup,
            ..Default::default()
        };

        let mut contact_group_not_empty = Label {
            remote_id: Some(not_empty_group_id.clone()),
            name: "contact_group_not_empty".to_owned(),
            label_type: LabelType::ContactGroup,
            ..Default::default()
        };

        let mut contact1 = contact!(remote_id: cid!("123"), name: "Barbara Fox".to_string());
        let mut contact2 = contact!(remote_id: cid!("456"), name: "Stevie Wonder".to_string());

        let mut contact1_email = contact_email!(remote_id: ceid!("ceid1"), label_ids: Labels::new(vec![not_empty_group_id.clone()]), remote_contact_id: contact1.remote_id.clone());

        let mut contact2_email = contact_email!(remote_id: ceid!("ceid2"), label_ids: Labels::new(vec![not_empty_group_id.clone()]), remote_contact_id: contact2.remote_id.clone());

        let tx = tether.transaction().await.unwrap();

        contact_group_empty.save(&tx).await.unwrap();
        contact_group_not_empty.save(&tx).await.unwrap();
        contact1.save(&tx).await.unwrap();
        contact2.save(&tx).await.unwrap();
        contact1_email.save(&tx).await.unwrap();
        contact2_email.save(&tx).await.unwrap();

        tx.commit().await.unwrap();

        assert_eq!(
            ContactEmail::count_in_contact_group(empty_group_id, &tether)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            ContactEmail::count_in_contact_group(not_empty_group_id, &tether)
                .await
                .unwrap(),
            2
        );

        assert_eq!(
            ContactEmail::count_in_contact_group_by_name("contact_group_empty".to_owned(), &tether)
                .await
                .unwrap(),
            Some(0)
        );
        assert_eq!(
            ContactEmail::count_in_contact_group_by_name(
                "contact_group_not_empty".to_owned(),
                &tether
            )
            .await
            .unwrap(),
            Some(2)
        );
        assert_eq!(
            ContactEmail::count_in_contact_group_by_name(
                "contact_group_unknown".to_owned(),
                &tether
            )
            .await
            .unwrap(),
            None
        );
    }
}

mod contact_watcher {
    use stash::{orm::Model, params};

    use crate::{cid, contact, models::Contact, tests::common::new_core_test_connection};

    #[tokio::test]
    async fn test_contact_list_watcher() {
        let mut tether = new_core_test_connection().await.connection();
        let mut contact = contact!(remote_id: cid!("123"), name: "Barbara Fox".to_string());
        let tx = tether.transaction().await.unwrap();
        contact.save(&tx).await.unwrap();
        tx.commit().await.unwrap();
        let (_, list_receiver) = Contact::watch_contact_list(tether.stash()).await.unwrap();
        let list_receiver = list_receiver.receiver;

        // Rename contact
        let tx = tether.transaction().await.unwrap();
        contact.name = "Barbara Lox".to_string();
        contact.save(&tx).await.unwrap();
        tx.commit().await.unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        // Soft delete contact
        let tx = tether.transaction().await.unwrap();
        contact.deleted = true;
        contact.save(&tx).await.unwrap();
        tx.commit().await.unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        // Soft undelete contact
        let tx = tether.transaction().await.unwrap();
        contact.deleted = false;
        contact.save(&tx).await.unwrap();
        tx.commit().await.unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        // Hard delete contact
        let tx = tether.transaction().await.unwrap();
        tx.execute(
            "DELETE FROM contacts WHERE local_id = ?",
            params![contact.local_id],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let all_contacts = Contact::find("", vec![], &tether).await.unwrap();
        assert_eq!(all_contacts.len(), 0);

        assert!(list_receiver.recv_async().await.is_ok());
    }
}
