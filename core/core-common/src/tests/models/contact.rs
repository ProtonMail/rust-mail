mod contact_list {
    use crate::{
        contact, contact_email,
        datatypes::{
            AvatarInformation, ContactEmailItem, ContactItem, ContactItemType, GroupedContacts,
        },
        lid,
        models::{Contact, ContactEmail},
        rid,
        tests::common::new_core_test_connection,
    };
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    #[test_case(vec![], vec![]; "TEST 0 Empty")]
    #[test_case(vec![contact!(local_id: lid!(123), name: "Barbara Lox".to_string())], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            item: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "Barbara Lox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "BL".to_string(),
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
    ], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            item: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "Barbara Fox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "BF".to_string(),
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
                            text: "BL".to_string(),
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
    ], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            item: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "🦊 Barbara Fox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "BF".to_string(),
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
                            text: "BL".to_string(),
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
    ], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            item: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id: 123.into(),
                        name: "❤️‍🔥 Barbara Fox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "BF".to_string(),
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
                            text: "BL".to_string(),
                            color: "#4989FF".to_string(),
                        },
                        emails: vec![],
                    },
                ),
            ],
        },
        GroupedContacts {
            grouped_by: "🙀".to_string(),
            item: vec![
                ContactItemType::Contact(
                    ContactItem {
                        local_id:  123.into(),
                        name: "🙀".to_string(),
                        avatar_information: AvatarInformation {
                            text: "🙀".to_string(),
                            color: "#52006A".to_string(),
                        },
                        emails: vec![],
                    },
                ),
            ],
        }
    ]; "TEST 4 Mutliple groups")]
    fn test_grouped_contacts(contacts: Vec<Contact>, expected: Vec<GroupedContacts>) {
        let result = GroupedContacts::from_contacts(contacts);
        assert_eq!(result, expected);
    }

    #[test_case(vec![
        contact_email!(remote_id: rid!("3"), email: "barbara1984@yahoo.com".to_string(), display_order: 3),
        contact_email!(remote_id: rid!("1"), email: "barbara@fox.us".to_string(), display_order: 2),
        contact_email!(remote_id: rid!("2"), email: "bfox@proton.me".to_string(), display_order: 1),
    ], vec![
    GroupedContacts {
        grouped_by: "B".to_string(),
        item: vec![
            ContactItemType::Contact(
                ContactItem {
                    local_id: 1.into(),
                    name: "Barbara Fox".to_string(),
                    avatar_information: AvatarInformation {
                        text: "BF".to_string(),
                        color: "#1ED19C".to_string(),
                    },
                    emails: vec![
                        ContactEmailItem {
                            local_id: 3.into(),
                            email: "bfox@proton.me".to_string(),
                        },
                        ContactEmailItem {
                            local_id: 2.into(),
                            email: "barbara@fox.us".to_string(),
                        },
                        ContactEmailItem {
                            local_id: 1.into(),
                            email: "barbara1984@yahoo.com".to_string(),
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
        let stash = new_core_test_connection().await;
        let mut contact = contact!(remote_id: rid!("123"), name: "Barbara Fox".to_string());
        contact.save_using(&stash).await.unwrap();

        for mut email in emails {
            email.remote_contact_id = contact.remote_id.clone();
            email.save_using(&stash).await.unwrap();
        }

        let result = Contact::contact_list(&stash).await.unwrap();
        assert_eq!(result, expected);
    }
}
