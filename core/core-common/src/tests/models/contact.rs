#![allow(clippy::needless_pass_by_value)]
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
    ], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            item: vec![
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
    ], vec![
        GroupedContacts {
            grouped_by: "B".to_string(),
            item: vec![
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
    ], vec![
        GroupedContacts {
            grouped_by: "#".to_string(),
            item: vec![
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
            item: vec![
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
                        text: "B".to_string(),
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
        contact.save(&stash).await.unwrap();

        for mut email in emails {
            email.remote_contact_id = contact.remote_id.clone();
            email.save(&stash).await.unwrap();
        }

        let result = Contact::contact_list(&stash).await.unwrap();
        assert_eq!(result, expected);
    }
}

mod contact_watcher {
    use stash::{exports::Action, orm::Model, params, stash::Interface};

    use crate::{contact, models::Contact, rid, tests::common::new_core_test_connection};

    #[tokio::test]
    async fn test_contact_list_watcher() {
        let stash = new_core_test_connection().await;
        let mut contact = contact!(remote_id: rid!("123"), name: "Barbara Fox".to_string());
        contact.save(&stash).await.unwrap();
        let (_, list_receiver) = Contact::watch_contact_list(&stash).await.unwrap();
        let stash_reciever = stash.subscribe().await.unwrap();

        // Rename contact
        let tx = stash.transaction().await.unwrap();
        contact.name = "Barbara Lox".to_string();
        contact.save(&tx).await.unwrap();
        tx.commit().await.unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        let notification = stash_reciever.recv_async().await.unwrap();
        assert_eq!(notification.table, "contacts".to_string());
        assert_eq!(notification.action, Action::SQLITE_UPDATE);

        // Soft delete contact
        let tx = stash.transaction().await.unwrap();
        contact.deleted = true;
        contact.save(&tx).await.unwrap();
        tx.commit().await.unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        let notification = stash_reciever.recv_async().await.unwrap();

        assert_eq!(notification.table, "contacts".to_string());
        assert_eq!(notification.action, Action::SQLITE_UPDATE);

        // Soft undelete contact
        let tx = stash.transaction().await.unwrap();
        contact.deleted = false;
        contact.save(&tx).await.unwrap();
        tx.commit().await.unwrap();

        assert!(list_receiver.recv_async().await.is_ok());

        let notification = stash_reciever.recv_async().await.unwrap();

        assert_eq!(notification.table, "contacts".to_string());
        assert_eq!(notification.action, Action::SQLITE_UPDATE);

        // Hard delete contact
        let tx = stash.transaction().await.unwrap();
        tx.execute(
            "DELETE FROM contacts WHERE local_id = ?",
            params![contact.local_id],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let all_contacts = Contact::find("", vec![], &stash, None).await.unwrap();
        assert_eq!(all_contacts.len(), 0);

        assert!(list_receiver.recv_async().await.is_ok());

        let notification = stash_reciever.recv_async().await.unwrap();

        assert_eq!(notification.table, "contacts".to_string());
        assert_eq!(notification.action, Action::SQLITE_DELETE);
    }
}
