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
    use proton_api_core::services::proton::LabelId;
    use stash::stash::StashError;
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
        tether
            .tx::<_, _, StashError>(async |tx| {
                contact.save(tx).await?;

                for mut email in emails {
                    email.remote_contact_id = contact.remote_id.clone();
                    email.save(tx).await?;
                }
                Ok(())
            })
            .await
            .expect("commit failed");

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

        tether
            .tx::<_, _, StashError>(async |tx| {
                contact_group_empty.save(tx).await.unwrap();
                contact_group_not_empty.save(tx).await.unwrap();
                contact1.save(tx).await.unwrap();
                contact2.save(tx).await.unwrap();
                contact1_email.save(tx).await.unwrap();
                contact2_email.save(tx).await.unwrap();
                Ok(())
            })
            .await
            .unwrap();

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
        let stash = new_core_test_connection().await;
        let mut tether = stash.connection();
        let mut contact = contact!(remote_id: cid!("123"), name: "Barbara Fox".to_string());
        tether.tx(async |tx| contact.save(tx).await).await.unwrap();
        let (_, list_receiver) = Contact::watch_contact_list(&stash).await.unwrap();
        let list_receiver = list_receiver.receiver;

        // Rename contact
        tether
            .tx(async |tx| {
                contact.name = "Barbara Lox".to_string();
                contact.save(tx).await
            })
            .await
            .unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        // Soft delete contact
        tether
            .tx(async |tx| {
                contact.deleted = true;
                contact.save(tx).await
            })
            .await
            .unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        // Soft undelete contact
        tether
            .tx(async |tx| {
                contact.deleted = false;
                contact.save(tx).await
            })
            .await
            .unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        // Hard delete contact
        tether
            .tx(async |tx| {
                tx.execute(
                    "DELETE FROM contacts WHERE local_id = ?",
                    params![contact.local_id],
                )
                .await
            })
            .await
            .unwrap();
        let all_contacts = Contact::find("", vec![], &tether).await.unwrap();
        assert_eq!(all_contacts.len(), 0);

        assert!(list_receiver.recv_async().await.is_ok());
    }
}

mod contact_suggestions {
    use crate::{
        ceid, cid, contact, contact_email,
        datatypes::{
            AvatarInformation, ContactEmailItem, ContactSuggestion, ContactSuggestionKind,
            ContactSuggestions, DeviceContact, DeviceContactSuggestion, LabelType,
        },
        device_contact, label, label_id, labels, lid,
        models::{Contact, Label},
        tests::common::new_core_test_connection,
    };
    use test_case::test_case;

    fn pretty_assert(expected: Vec<ContactSuggestion>) -> impl Fn(Vec<ContactSuggestion>) {
        move |actual| pretty_assertions::assert_eq!(actual, expected)
    }

    struct TestCase {
        contacts: Vec<Contact>,
        contact_groups: Vec<Label>,
        device_contacts: Vec<DeviceContact>,
    }

    #[test_case(TestCase { contacts: vec![], contact_groups: vec![], device_contacts: vec![]} => Vec::<ContactSuggestion>::new() ; "TEST 0 - Empty")]
    #[test_case(TestCase {
        contacts: vec![ contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
            contact_email!(local_id: lid!(123), is_proton: false, email: "barbara@lox.com".to_string(), last_used_time: 1)
        ])],
        contact_groups:  vec![],
        device_contacts: vec![]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@lox.com".to_string(),
                is_proton: false,
                last_used_time: 1
            })
        }
     ]) ; "TEST 1 - Single contact")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: false, email: "barbara@lox.com".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 1)
            ])
        ],
        contact_groups:  vec![],
        device_contacts: vec![]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/234".to_string(),
            name: "Michael Scott".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#213474".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 234.into(),
                email: "m.scott@pm.me".to_string(),
                is_proton: true,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@lox.com".to_string(),
                is_proton: false,
                last_used_time: 1
            })

        }
     ]) ; "TEST 2 - Proton mails go first")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 2)
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3)
            ])
        ],
        contact_groups:  vec![],
        device_contacts: vec![]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/234".to_string(),
            name: "Michael Scott".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#213474".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 234.into(),
                email: "m.scott@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@pm.me".to_string(),
                is_proton: true,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "contact/456".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 456.into(),
                email: "jake.peralta@99.com".to_string(),
                is_proton: false,
                last_used_time: 3
            })
        }
     ]) ; "TEST 3 - Frequently used mails go first")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 2)
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".to_string(), last_used_time: 2)
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3)
            ]),
        ],
        contact_groups:  vec![],
        device_contacts: vec![]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/678".to_string(),
            name: "Jason Mendoza".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#3CBB3A".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 678.into(),
                email: "jianyu.li@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/234".to_string(),
            name: "Michael Scott".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#213474".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 234.into(),
                email: "m.scott@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@pm.me".to_string(),
                is_proton: true,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "contact/456".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 456.into(),
                email: "jake.peralta@99.com".to_string(),
                is_proton: false,
                last_used_time: 3
            })
        }
     ]) ; "TEST 4 - In the end lexicographic order is used")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3, label_ids: labels!("m.schur.productions")),
                // Only first email was added to the group
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".to_string(), last_used_time: 1)
            ]),
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
        ],
        device_contacts: vec![]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/678".to_string(),
            name: "Jason Mendoza".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#3CBB3A".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 678.into(),
                email: "jianyu.li@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/234".to_string(),
            name: "Michael Scott".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#213474".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 234.into(),
                email: "m.scott@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@pm.me".to_string(),
                is_proton: true,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "contact/456".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 456.into(),
                email: "jake.peralta@99.com".to_string(),
                is_proton: false,
                last_used_time: 3
            })
        },
        ContactSuggestion {
            key: "contact/112".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 112.into(),
                email: "harvey@jp.com".to_string(),
                is_proton: false,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "group/910".to_string(),
            name: "M. Schur Productions".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#52006A".to_string()
            },
            kind: ContactSuggestionKind::ContactGroup(vec![
                ContactEmailItem {
                    local_id: 678.into(),
                    email: "jianyu.li@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 234.into(),
                    email: "m.scott@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 456.into(),
                    email: "jake.peralta@99.com".to_string(),
                    is_proton: false,
                    last_used_time: 3
                },
            ])
        }
     ]) ; "TEST 5 - Contact groups")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3, label_ids: labels!("m.schur.productions")),
                // Only first email was added to the group
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".to_string(), last_used_time: 1)
            ]),
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
        ],
        device_contacts: vec![
            device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "molly@family.com".to_string(),
                "badass@aunt.com".to_string(),
            ])
        ]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/678".to_string(),
            name: "Jason Mendoza".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#3CBB3A".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 678.into(),
                email: "jianyu.li@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/234".to_string(),
            name: "Michael Scott".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#213474".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 234.into(),
                email: "m.scott@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@pm.me".to_string(),
                is_proton: true,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "contact/456".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 456.into(),
                email: "jake.peralta@99.com".to_string(),
                is_proton: false,
                last_used_time: 3
            })
        },
        ContactSuggestion {
            key: "contact/112".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 112.into(),
                email: "harvey@jp.com".to_string(),
                is_proton: false,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "device-contact-email/000-0".to_string(),
            name: "Aunt Molly".to_string(),
            avatar_information: AvatarInformation {
                text: "A".to_string(),
                color: "#52006A".to_string(),
            },
            kind: ContactSuggestionKind::DeviceContact(DeviceContactSuggestion {
                email: "molly@family.com".to_string()
            })
        },
        // Device contact emails are not sorted by email address
        ContactSuggestion {
            key: "device-contact-email/000-1".to_string(),
            name: "Aunt Molly".to_string(),
            avatar_information: AvatarInformation {
                text: "A".to_string(),
                color: "#52006A".to_string(),
            },
            kind: ContactSuggestionKind::DeviceContact(DeviceContactSuggestion {
                email: "badass@aunt.com".to_string()
            })
        },
        ContactSuggestion {
            key: "group/910".to_string(),
            name: "M. Schur Productions".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#52006A".to_string()
            },
            kind: ContactSuggestionKind::ContactGroup(vec![
                ContactEmailItem {
                    local_id: 678.into(),
                    email: "jianyu.li@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 234.into(),
                    email: "m.scott@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 456.into(),
                    email: "jake.peralta@99.com".to_string(),
                    is_proton: false,
                    last_used_time: 3
                },
            ])
        }
     ]) ; "TEST 6 - Contact groups and device contacts are in the end, sorted by name")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3, label_ids: labels!("m.schur.productions")),
                // Only first email was added to the group
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".to_string(), last_used_time: 1)
            ]),
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
        ],
        device_contacts: vec![
            device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "molly@family.com".to_string(),
            ]),
            device_contact!(key: "001".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "badass@aunt.com".to_string(),
            ])
        ]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/678".to_string(),
            name: "Jason Mendoza".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#3CBB3A".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 678.into(),
                email: "jianyu.li@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/234".to_string(),
            name: "Michael Scott".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#213474".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 234.into(),
                email: "m.scott@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@pm.me".to_string(),
                is_proton: true,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "contact/456".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 456.into(),
                email: "jake.peralta@99.com".to_string(),
                is_proton: false,
                last_used_time: 3
            })
        },
        ContactSuggestion {
            key: "contact/112".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 112.into(),
                email: "harvey@jp.com".to_string(),
                is_proton: false,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "device-contact-email/000-0".to_string(),
            name: "Aunt Molly".to_string(),
            avatar_information: AvatarInformation {
                text: "A".to_string(),
                color: "#52006A".to_string(),
            },
            kind: ContactSuggestionKind::DeviceContact(DeviceContactSuggestion {
                email: "molly@family.com".to_string()
            })
        },
        ContactSuggestion {
            key: "device-contact-email/001-0".to_string(),
            name: "Aunt Molly".to_string(),
            avatar_information: AvatarInformation {
                text: "A".to_string(),
                color: "#52006A".to_string(),
            },
            kind: ContactSuggestionKind::DeviceContact(DeviceContactSuggestion {
                email: "badass@aunt.com".to_string()
            })
        },
        ContactSuggestion {
            key: "group/910".to_string(),
            name: "M. Schur Productions".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#52006A".to_string()
            },
            kind: ContactSuggestionKind::ContactGroup(vec![
                ContactEmailItem {
                    local_id: 678.into(),
                    email: "jianyu.li@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 234.into(),
                    email: "m.scott@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 456.into(),
                    email: "jake.peralta@99.com".to_string(),
                    is_proton: false,
                    last_used_time: 3
                },
            ])
        }
     ]) ; "TEST 7 - Device Contacts are sorted by name and ids")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3, label_ids: labels!("m.schur.productions")),
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".to_string(), last_used_time: 1)
            ]),
            contact!(name: "Detective Peralta".to_string(), contact_emails: vec![
                // User has two contacts pointing to the same email
                contact_email!(local_id: lid!(999), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3)
            ])
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
        ],
        device_contacts: vec![
            device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "molly@family.com".to_string(),
            ]),
            device_contact!(key: "001".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "badass@aunt.com".to_string(),
            ]),
            // User has also a device contact that duplicates proton contact
            device_contact!(key: "002".to_string(), name: "Boss".to_string(), emails: vec![
                "m.scott@pm.me".to_string()
            ]),
            device_contact!(key: "003".to_string(), name: "Aunt Molly (Copy)".to_string(), emails: vec![
                "badass@aunt.com".to_string(),
            ]),
        ]
     } => using pretty_assert(vec![
        ContactSuggestion {
            key: "contact/678".to_string(),
            name: "Jason Mendoza".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#3CBB3A".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 678.into(),
                email: "jianyu.li@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/234".to_string(),
            name: "Michael Scott".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#213474".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 234.into(),
                email: "m.scott@pm.me".to_string(),
                is_proton: true,
                last_used_time: 2
            })
        },
        ContactSuggestion {
            key: "contact/123".to_string(),
            name: "Barbara Lox".to_string(),
            avatar_information: AvatarInformation {
                text: "B".to_string(),
                color: "#A839A4".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 123.into(),
                email: "barbara@pm.me".to_string(),
                is_proton: true,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "contact/999".to_string(),
            name: "Detective Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "D".to_string(),
                color: "#415DF0".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 999.into(),
                email: "jake.peralta@99.com".to_string(),
                is_proton: false,
                last_used_time: 3
            })
        },
        ContactSuggestion {
            key: "contact/112".to_string(),
            name: "Jake Peralta".to_string(),
            avatar_information: AvatarInformation {
                text: "J".to_string(),
                color: "#9C89FF".to_string()
            },
            kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                local_id: 112.into(),
                email: "harvey@jp.com".to_string(),
                is_proton: false,
                last_used_time: 1
            })
        },
        ContactSuggestion {
            key: "device-contact-email/000-0".to_string(),
            name: "Aunt Molly".to_string(),
            avatar_information: AvatarInformation {
                text: "A".to_string(),
                color: "#52006A".to_string(),
            },
            kind: ContactSuggestionKind::DeviceContact(DeviceContactSuggestion {
                email: "molly@family.com".to_string()
            })
        },
        ContactSuggestion {
            key: "device-contact-email/001-0".to_string(),
            name: "Aunt Molly".to_string(),
            avatar_information: AvatarInformation {
                text: "A".to_string(),
                color: "#52006A".to_string(),
            },
            kind: ContactSuggestionKind::DeviceContact(DeviceContactSuggestion {
                email: "badass@aunt.com".to_string()
            })
        },
        ContactSuggestion {
            key: "group/910".to_string(),
            name: "M. Schur Productions".to_string(),
            avatar_information: AvatarInformation {
                text: "M".to_string(),
                color: "#52006A".to_string()
            },
            kind: ContactSuggestionKind::ContactGroup(vec![
                ContactEmailItem {
                    local_id: 678.into(),
                    email: "jianyu.li@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 234.into(),
                    email: "m.scott@pm.me".to_string(),
                    is_proton: true,
                    last_used_time: 2
                },
                ContactEmailItem {
                    local_id: 456.into(),
                    email: "jake.peralta@99.com".to_string(),
                    is_proton: false,
                    last_used_time: 3
                },
            ])
        }
     ]) ; "TEST 8 - contacts are deduplicated")]
    fn test_contact_suggestions(test_case: TestCase) -> Vec<ContactSuggestion> {
        ContactSuggestions::from_contacts_and_device_contacts(
            test_case.contacts,
            test_case.contact_groups,
            test_case.device_contacts,
        )
        .all()
        .to_vec()
    }

    fn pretty_assert_emails(expected: Vec<&'static str>) -> impl Fn(Vec<ContactSuggestion>) {
        move |actual| {
            let actual = actual
                .into_iter()
                .map(|suggestion| match suggestion.kind {
                    ContactSuggestionKind::ContactItem(contact_email_item) => {
                        format!("{} <{}>", suggestion.name, contact_email_item.email)
                    }
                    ContactSuggestionKind::DeviceContact(device_contact_suggestion) => {
                        format!("{} <{}>", suggestion.name, device_contact_suggestion.email)
                    }
                    ContactSuggestionKind::ContactGroup(vec) => {
                        format!("{} ({} emails)", suggestion.name, vec.len())
                    }
                })
                .collect::<Vec<_>>();
            pretty_assertions::assert_eq!(actual, expected);
        }
    }

    fn empty_test_case() -> TestCase {
        TestCase {
            contacts: vec![],
            contact_groups: vec![],
            device_contacts: vec![],
        }
    }
    fn filtering_test_case() -> TestCase {
        TestCase {
            contacts: vec![
                contact!(name: "Barbara Lox".to_string(), remote_id: cid!("lox"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("123"), is_proton: true, email: "barbara@pm.me".to_string(), last_used_time: 1)
                ]),
                contact!(name: "Michael Scott".to_string(), remote_id: cid!("scott"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("234"), is_proton: true, email: "m.scott@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
                ]),
                contact!(name: "Jason Mendoza".to_string(), remote_id: cid!("mendoza"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("678"), is_proton: true, email: "jianyu.li@pm.me".to_string(), last_used_time: 2, label_ids: labels!("m.schur.productions"))
                ]),
                contact!(name: "Jake Peralta".to_string(), remote_id: cid!("peralta"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("456"), is_proton: false, email: "jake.peralta@99.com".to_string(), last_used_time: 3, label_ids: labels!("m.schur.productions")),
                    contact_email!(remote_id: ceid!("112"), is_proton: false, email: "harvey@jp.com".to_string(), last_used_time: 1)
                ]),
            ],
            contact_groups: vec![
                label!(remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
            ],
            device_contacts: vec![
                device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                    "molly@family.com".to_string(),
                ]),
                device_contact!(key: "001".to_string(), name: "Molly".to_string(), emails: vec![
                    "badass@aunt.com".to_string(),
                ]),
            ],
        }
    }

    #[test_case("pe", empty_test_case() => using pretty_assert_emails(vec![]) ; "TEST 0A - empty contact book")]
    #[test_case("", empty_test_case() => using pretty_assert_emails(vec![]) ; "TEST 0B - empty query")]
    #[test_case("", filtering_test_case() => using pretty_assert_emails(vec![]) ; "TEST 0C - empty query with non-empty contact book")]
    #[test_case("Lox", filtering_test_case() => using pretty_assert_emails(vec![ "Barbara Lox <barbara@pm.me>" ]) ; "TEST 1 - filtering by name")]
    #[test_case("lox", filtering_test_case() => using pretty_assert_emails(vec![ "Barbara Lox <barbara@pm.me>" ]) ; "TEST 2 - filtering case insensitive")]
    #[test_case("jianyu", filtering_test_case() => using pretty_assert_emails(vec![ "Jason Mendoza <jianyu.li@pm.me>" ]) ; "TEST 3 - filtering by email")]
    #[test_case("Jake", filtering_test_case() => using pretty_assert_emails(vec![ "Jake Peralta <jake.peralta@99.com>", "Jake Peralta <harvey@jp.com>" ]) ; "TEST 4 - filtering by name, contact has multiple emails")]
    #[test_case("Schur", filtering_test_case() => using pretty_assert_emails(vec![ "M. Schur Productions (3 emails)"]) ; "TEST 5 - filtering by name, contact group returned")]
    #[test_case("aunt", filtering_test_case() => using pretty_assert_emails(vec![
        "Aunt Molly <molly@family.com>",
        "Molly <badass@aunt.com>",
    ]) ; "TEST 6 - device contacts filtered by both name and email")]
    #[test_case("m", filtering_test_case() => using pretty_assert_emails(vec![
        "Jason Mendoza <jianyu.li@pm.me>",
        "Michael Scott <m.scott@pm.me>",
        "Barbara Lox <barbara@pm.me>",
        "Jake Peralta <jake.peralta@99.com>",
        "Jake Peralta <harvey@jp.com>",
        "Aunt Molly <molly@family.com>",
        "M. Schur Productions (3 emails)",
        "Molly <badass@aunt.com>",
    ]) ; "TEST 7 - finding all")]
    #[tokio::test]
    async fn test_contact_suggestions_filtering(
        query: &str,
        mut test_case: TestCase,
    ) -> Vec<ContactSuggestion> {
        let mut tether = new_core_test_connection().await.connection();
        tether
            .tx::<_, _, stash::stash::StashError>(async |tx| {
                for contact in &mut test_case.contacts {
                    contact.save(tx).await.unwrap();
                    for email in &mut contact.contact_emails {
                        email.remote_contact_id = contact.remote_id.clone();
                        email.save(tx).await.unwrap();
                    }
                }
                for label in &mut test_case.contact_groups {
                    label.save(tx).await.unwrap();
                }
                Ok(())
            })
            .await
            .expect("commit failed");

        let suggestions = Contact::contact_suggestions(test_case.device_contacts, &tether)
            .await
            .unwrap();

        suggestions.filtered(query)
    }
}
