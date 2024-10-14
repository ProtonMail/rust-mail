mod contact_list {
    use crate::{
        contact,
        datatypes::{AvatarInformation, ContactItem, ContactItemType, GroupedContacts, LocalId},
        lid,
        models::Contact,
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
}
