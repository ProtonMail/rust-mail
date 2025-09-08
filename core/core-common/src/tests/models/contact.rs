#![allow(clippy::needless_pass_by_value)]
#![allow(unused_must_use)]

use crate::datatypes::{
    ContactEmailItem, ContactGroupItem, ContactItem, ContactItemType, ContactSuggestion,
    ContactSuggestionKind, GroupedContacts,
};
use std::fmt::Write as _;

fn display_email_item(
    ContactEmailItem {
        local_contact_id,
        email,
        is_proton,
        last_used_time,
        name,
        avatar_information,
    }: ContactEmailItem,
    out: &mut String,
) {
    write!(
        out,
        "{} ({})",
        avatar_information.text, avatar_information.color
    );
    write!(out, ": {name} <{email}>");

    if local_contact_id != 0.into() {
        write!(out, ",  local_contact_id: {local_contact_id}");
    }
    if is_proton {
        write!(out, ", Proton address");
    }
    if last_used_time.as_u64() != 0 {
        write!(out, ", last used: {last_used_time}");
    }
    writeln!(out);
}

fn display_suggestions(sug: Vec<ContactSuggestion>) -> String {
    let mut out = String::new();
    writeln!(out, "{} suggestions:", sug.len());
    for ContactSuggestion {
        key,
        name,
        avatar_information,
        kind,
    } in sug
    {
        writeln!(out, "\n{key}: {name}");
        match kind {
            ContactSuggestionKind::ContactItem(em) => {
                display_email_item(em, &mut out);
            }
            ContactSuggestionKind::DeviceContact(contact) => {
                writeln!(
                    out,
                    "{} ({}): <{}>",
                    avatar_information.text, avatar_information.color, contact.email
                );
            }
            ContactSuggestionKind::ContactGroup(items) => {
                for item in items {
                    display_email_item(item, &mut out);
                }
            }
        }
    }
    out
}

pub fn display_group(groups: Vec<GroupedContacts>) -> String {
    let mut out = String::new();
    writeln!(out, "{} keys:", groups.len());
    for GroupedContacts { grouped_by, items } in groups {
        writeln!(
            out,
            "\n{grouped_by} ({} {})",
            items.len(),
            if items.len() == 1 { "item" } else { "items" }
        );
        for item in items {
            match item {
                ContactItemType::Contact(ContactItem {
                    local_id: _,
                    name,
                    avatar_information,
                    emails,
                }) => {
                    // Contact A (#color): Name (1 address)
                    write!(
                        out,
                        "Contact {} ({}): {}",
                        avatar_information.text, avatar_information.color, name
                    );
                    writeln!(
                        out,
                        " ({} {})",
                        emails.len(),
                        if emails.len() == 1 {
                            "address"
                        } else {
                            "addresses"
                        }
                    );
                    for em in emails {
                        display_email_item(em, &mut out);
                    }
                }
                ContactItemType::Group(ContactGroupItem {
                    local_id: _,
                    name,
                    avatar_information,
                    contacts,
                }) => {
                    // Group A (#color): Name (1 address)
                    write!(
                        out,
                        "Group {} ({}): {}",
                        avatar_information.text, avatar_information.color, name
                    );
                    writeln!(
                        out,
                        " ({} {})",
                        contacts.len(),
                        if contacts.len() == 1 {
                            "address"
                        } else {
                            "addresses"
                        }
                    );
                    for em in contacts {
                        display_email_item(em, &mut out);
                    }
                }
            }
        }
    }
    out
}

mod contact_list {
    use crate::datatypes::Labels;
    use crate::models::tests::contact::display_group;
    use crate::{
        ceid, cid, contact, contact_email,
        datatypes::{GroupedContacts, LabelType},
        label, label_id, labels, lid,
        models::{Contact, ContactEmail, Label},
        tests::common::new_core_test_connection,
    };
    use pretty_assertions::assert_eq;
    use proton_core_api::services::proton::LabelId;
    use stash::orm::Model;
    use stash::stash::StashError;
    use test_case::test_case;

    #[test_case(vec![], vec![]
    ,0; "TEST 0 Empty")]
    #[test_case(vec![contact!(local_id: lid!(123), name: "Barbara Lox".to_string())], vec![]
    ,1; "TEST 1 Basic")]
    #[test_case(vec![
        contact!(local_id: lid!(123), name: "Barbara Lox".to_string()),
        contact!(local_id: lid!(123), name: "Barbara Fox".to_string())
    ],
        vec![]
    ,2; "TEST 2 Alphabetical order")]
    #[test_case(vec![
        contact!(local_id: lid!(123), name: "🐂 Barbara Lox".to_string()),
        contact!(local_id: lid!(123), name: "🦊 Barbara Fox".to_string())
    ], vec![]
    ,3; "TEST 3 With emojis")]
    #[test_case(vec![
        contact!(local_id: lid!(123), name: "🙀".to_string()),
        contact!(local_id: lid!(123), name: "🙀 Barbara Lox".to_string()),
        contact!(local_id: lid!(123), name: "❤️‍🔥 Barbara Fox".to_string())
    ], vec![]
    ,4 ; "TEST 4 Mutliple groups")]
    #[test_case(vec![
        contact!(local_id: lid!(123), label_ids: labels!("family"), name: "Mom".to_string()),
        contact!(local_id: lid!(124), label_ids: labels!("family"), name: "Dad".to_string()),
        contact!(local_id: lid!(125), label_ids: labels!("family"), name: "Sister".to_string())
    ], vec![
        label!(local_id: lid!(100), remote_id: Some(label_id!("family")), name: "Family".to_string(), label_type: LabelType::ContactGroup)
    ]
    ,5; "TEST 5 Contact groups (labels)")]
    fn test_grouped_contacts(contacts: Vec<Contact>, groups: Vec<Label>, test_number: u32) {
        let groups = GroupedContacts::from_contacts_and_groups(contacts, groups);
        insta::assert_snapshot!(
            format!("test_grouped_contacts_{}", test_number),
            display_group(groups)
        );
    }

    #[tokio::test]
    async fn test_grouped_contacts_emails_order() {
        let emails = vec![
            contact_email!(remote_id: ceid!("3"), email: "barbara1984@yahoo.com".into(), display_order: 3),
            contact_email!(remote_id: ceid!("1"), email: "barbara@fox.us".into(), display_order: 2),
            contact_email!(remote_id: ceid!("2"), email: "bfox@proton.me".into(), display_order: 1, is_proton: true),
        ];

        let mut tether = new_core_test_connection().await.connection().await.unwrap();
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
        insta::assert_snapshot!(display_group(result));
    }

    #[tokio::test]
    async fn test_count_email_group_count() {
        let mut tether = new_core_test_connection().await.connection().await.unwrap();

        let empty_group_id = LabelId::from("l1");
        let not_empty_group_id = LabelId::from("l2");
        let mut contact_group_empty = Label {
            remote_id: Some(empty_group_id.clone()),
            name: "contact_group_empty".to_owned(),
            label_type: LabelType::ContactGroup,
            ..Label::test_default()
        };

        let mut contact_group_not_empty = Label {
            remote_id: Some(not_empty_group_id.clone()),
            name: "contact_group_not_empty".to_owned(),
            label_type: LabelType::ContactGroup,
            ..Label::test_default()
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
        let mut tether = stash.connection().await.unwrap();
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
            ContactSuggestions, DeviceContact, LabelType,
        },
        device_contact, label, label_id, labels, lid,
        models::{Contact, Label},
        tests::common::new_core_test_connection,
    };
    use stash::orm::Model;
    use test_case::test_case;

    use super::display_suggestions;

    #[derive(Default)]
    struct TestCase {
        contacts: Vec<Contact>,
        contact_groups: Vec<Label>,
        device_contacts: Vec<DeviceContact>,
    }

    #[test_case(TestCase::default()
    ,0; "TEST 0 - Empty")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), 
                    contact_emails: vec![contact_email!(local_id: lid!(123), is_proton: false, email: "barbara@lox.com".into(), last_used_time: 1.into())
                ])],
        ..Default::default()
     }
    ,1; "TEST 1 - Single contact")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: false, email: "barbara@lox.com".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 1.into())
            ])
        ],
        ..Default::default()
     }
    ,2; "TEST 2 - Proton mails go first")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 2.into())
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into())
            ])
        ],
        ..Default::default()
     }
    ,3; "TEST 3 - Frequently used mails go first")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 2.into())
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".into(), last_used_time: 2.into())
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into())
            ]),
        ],
        ..Default::default()
     }
    ,4; "TEST 4 - In the end lexicographic order is used")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into(), label_ids: labels!("m.schur.productions")),
                // Only first email was added to the group
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".into(), last_used_time: 1.into())
            ]),
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".into(), label_type: LabelType::ContactGroup),
        ],
        ..Default::default()
     }
    ,5; "TEST 5 - Contact groups")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into(), label_ids: labels!("m.schur.productions")),
                // Only first email was added to the group
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".into(), last_used_time: 1.into())
            ]),
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".into(), label_type: LabelType::ContactGroup),
        ],
        device_contacts: vec![
            device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "molly@family.com".into(),
                "badass@aunt.com".into(),
            ])
        ]
     }
    ,6; "TEST 6 - Contact groups and device contacts are in the end, sorted by name")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".into(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into(), label_ids: labels!("m.schur.productions")),
                // Only first email was added to the group
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".into(), last_used_time: 1.into())
            ]),
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
        ],
        device_contacts: vec![
            device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "molly@family.com".into(),
            ]),
            device_contact!(key: "001".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "badass@aunt.com".into(),
            ])
        ]
     }
    ,7; "TEST 7 - Device Contacts are sorted by name and ids")]
    #[test_case(TestCase {
        contacts: vec![
            contact!(name: "Barbara Lox".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(123), is_proton: true, email: "barbara@pm.me".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Michael Scott".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(234), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jason Mendoza".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(678), is_proton: true, email: "jianyu.li@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
            ]),
            contact!(name: "Jake Peralta".to_string(), contact_emails: vec![
                contact_email!(local_id: lid!(456), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into(), label_ids: labels!("m.schur.productions")),
                contact_email!(local_id: lid!(112), is_proton: false, email: "harvey@jp.com".into(), last_used_time: 1.into())
            ]),
            contact!(name: "Detective Peralta".to_string(), contact_emails: vec![
                // User has two contacts pointing to the same email
                contact_email!(local_id: lid!(999), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into())
            ])
        ],
        contact_groups: vec![
            label!(local_id: lid!(910), remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
        ],
        device_contacts: vec![
            device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "molly@family.com".into(),
            ]),
            device_contact!(key: "001".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                "badass@aunt.com".into(),
            ]),
            // User has also a device contact that duplicates proton contact
            device_contact!(key: "002".to_string(), name: "Boss".to_string(), emails: vec![
                "m.scott@pm.me".into()
            ]),
            device_contact!(key: "003".to_string(), name: "Aunt Molly (Copy)".to_string(), emails: vec![
                "badass@aunt.com".into(),
            ]),
        ]
     }
    ,8; "TEST 8 - contacts are deduplicated")]
    fn test_contact_suggestions(test_case: TestCase, test_number: u32) {
        let res = ContactSuggestions::from_contacts_and_device_contacts(
            test_case.contacts,
            test_case.contact_groups,
            test_case.device_contacts,
        )
        .all()
        .to_vec();
        insta::assert_snapshot!(
            format!("test_contact_suggestions_{}", test_number),
            display_suggestions(res)
        );
    }

    #[test_case(ContactSuggestions::from(
             vec![
                ContactSuggestion {
                    key: "contact/234".to_string(),
                    name: "Michael Scott".to_string(),
                    avatar_information: AvatarInformation {
                        text: "M".to_string(),
                        color: "#213474".to_string()
                    },
                    kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                        name: "Michael Scott".to_string(),
                        avatar_information: AvatarInformation {
                            text: "M".to_string(),
                            color: "#213474".to_string()
                        },
                        local_contact_id: 234.into(),
                        email: "m.scott@pm.me".into(),
                        is_proton: true,
                        last_used_time: 2.into()
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
                        name: "Barbara Lox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#A839A4".to_string()
                        },
                        local_contact_id: 123.into(),
                        email: "barbara@pm.me".into(),
                        is_proton: true,
                        last_used_time: 1.into()
                    })
                },
            ]
        ),
        ContactSuggestions::from(
            vec![
               ContactSuggestion {
                   key: "contact/234".to_string(),
                   name: "Michael Scott".to_string(),
                   avatar_information: AvatarInformation {
                       text: "M".to_string(),
                       color: "#213474".to_string()
                   },
                   kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                       name: "Michael Scott".to_string(),
                       avatar_information: AvatarInformation {
                           text: "M".to_string(),
                           color: "#213474".to_string()
                       },
                       local_contact_id: 234.into(),
                       email: "m.scott@pm.me".into(),
                       is_proton: true,
                       last_used_time: 2.into()
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
                       name: "Barbara Lox".to_string(),
                       avatar_information: AvatarInformation {
                           text: "B".to_string(),
                           color: "#A839A4".to_string()
                       },
                       local_contact_id: 123.into(),
                       email: "barbara@pm.me".into(),
                       is_proton: true,
                       last_used_time: 1.into()
                   })
               },
           ]
        ), 0;
        "TEST0: Concat the same suggestions ends up in the initial list"
    )]
    #[test_case(ContactSuggestions::from (
             vec![
                ContactSuggestion {
                    key: "contact/235".to_string(),
                    name: "Michael Brogile".to_string(),
                    avatar_information: AvatarInformation {
                        text: "M".to_string(),
                        color: "#213474".to_string()
                    },
                    kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                        name: "Michael Brogile".to_string(),
                        avatar_information: AvatarInformation {
                            text: "M".to_string(),
                            color: "#213474".to_string()
                        },
                        local_contact_id: 234.into(),
                        email: "m.brogile@pm.me".into(),
                        is_proton: true,
                        last_used_time: 2.into()
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
                        name: "Barbara Lox".to_string(),
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#A839A4".to_string()
                        },
                        local_contact_id: 123.into(),
                        email: "barbara@pm.me".into(),
                        is_proton: true,
                        last_used_time: 1.into()
                    })
                },
            ]
        ),
         ContactSuggestions::from(
             vec![
                ContactSuggestion {
                    key: "contact/234".to_string(),
                    name: "Michael Scott".to_string(),
                    avatar_information: AvatarInformation {
                        text: "M".to_string(),
                        color: "#213474".to_string()
                    },
                    kind: ContactSuggestionKind::ContactItem(ContactEmailItem {
                        name: "Michael Scott".to_string(),
                        avatar_information: AvatarInformation {
                            text: "M".to_string(),
                            color: "#213474".to_string()
                        },
                        local_contact_id: 234.into(),
                        email: "m.scott@pm.me".into(),
                        is_proton: true,
                        last_used_time: 2.into()
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
                        avatar_information: AvatarInformation {
                            text: "B".to_string(),
                            color: "#A839A4".to_string()
                        },
                        name: "Barbara Lox".into(),
                        local_contact_id: 123.into(),
                        email: "barbara@pm.me".into(),
                        is_proton: true,
                        last_used_time: 1.into()
                    })
                },
            ]
        ), 1;
        "TEST1: Concat different suggestions are correctly deduplicated and sorted (other's at the end)"
    )]
    fn concat_contact_suggestions(
        mut one: ContactSuggestions,
        other: ContactSuggestions,
        test_number: u32,
    ) {
        one.concat(other);
        insta::assert_snapshot!(
            format!("concat_contact_suggestions_{}", test_number),
            display_suggestions(one.all().to_vec())
        );
    }

    fn pretty_assert_emails(expected: Vec<&'static str>) -> impl Fn(Vec<ContactSuggestion>) {
        move |actual| {
            let actual = actual
                .into_iter()
                .map(|suggestion| match suggestion.kind {
                    ContactSuggestionKind::ContactItem(contact_email_item) => {
                        format!(
                            "{} <{}>",
                            suggestion.name,
                            contact_email_item.email.as_clear_text_str()
                        )
                    }
                    ContactSuggestionKind::DeviceContact(device_contact_suggestion) => {
                        format!(
                            "{} <{}>",
                            suggestion.name,
                            device_contact_suggestion.email.as_clear_text_str()
                        )
                    }
                    ContactSuggestionKind::ContactGroup(vec) => {
                        format!("{} ({} emails)", suggestion.name, vec.len())
                    }
                })
                .collect::<Vec<_>>();
            pretty_assertions::assert_eq!(actual, expected);
        }
    }

    fn filtering_test_case() -> TestCase {
        TestCase {
            contacts: vec![
                contact!(name: "Barbara Lox".to_string(), remote_id: cid!("lox"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("123"), is_proton: true, email: "barbara@pm.me".into(), last_used_time: 1.into())
                ]),
                contact!(name: "Michael Scott".to_string(), remote_id: cid!("scott"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("234"), is_proton: true, email: "m.scott@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
                ]),
                contact!(name: "Jason Mendoza".to_string(), remote_id: cid!("mendoza"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("678"), is_proton: true, email: "jianyu.li@pm.me".into(), last_used_time: 2.into(), label_ids: labels!("m.schur.productions"))
                ]),
                contact!(name: "Jake Peralta".to_string(), remote_id: cid!("peralta"), contact_emails: vec![
                    contact_email!(remote_id: ceid!("456"), is_proton: false, email: "jake.peralta@99.com".into(), last_used_time: 3.into(), label_ids: labels!("m.schur.productions")),
                    contact_email!(remote_id: ceid!("112"), is_proton: false, email: "harvey@jp.com".into(), last_used_time: 1.into())
                ]),
            ],
            contact_groups: vec![
                label!(remote_id: Some(label_id!("m.schur.productions")), name: "M. Schur Productions".to_string(), label_type: LabelType::ContactGroup),
            ],
            device_contacts: vec![
                device_contact!(key: "000".to_string(), name: "Aunt Molly".to_string(), emails: vec![
                    "molly@family.com".into(),
                ]),
                device_contact!(key: "001".to_string(), name: "Molly".to_string(), emails: vec![
                    "badass@aunt.com".into(),
                ]),
            ],
        }
    }

    #[test_case("pe", TestCase::default() => using pretty_assert_emails(vec![]) ; "TEST 0A - empty contact book")]
    #[test_case("", TestCase::default() => using pretty_assert_emails(vec![]) ; "TEST 0B - empty query")]
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
        let mut tether = new_core_test_connection().await.connection().await.unwrap();
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
