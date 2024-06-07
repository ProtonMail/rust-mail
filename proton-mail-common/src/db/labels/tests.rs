use crate::db::{
    new_test_connection, with_tx, LabelColor, LocalLabel, LocalLabelId, MailSqliteConnectionImpl,
};
use proton_api_mail::domain::{ConversationCount, Label, LabelId, LabelType, MessageCount};

#[test]
fn test_remote_label_add() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let labels = test_labels();
        tx.create_remote_labels(labels.iter()).unwrap();
        compare_remote_labels_with_local(tx, labels.iter());
    })
}

#[test]
fn test_remote_label_add_duplicate() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let label = Label {
            id: LabelId::from("label_id2"),
            parent_id: None,
            name: "MyLabel".to_string(),
            path: None,
            color: "#ffffff".to_string(),
            label_type: LabelType::Label,
            notify: false,
            display: true,
            sticky: false,
            expanded: true,
            order: 0,
        };

        let local_id = tx.create_remote_label(&label).unwrap();
        let new_local_id = tx.create_remote_label(&label).unwrap();
        assert_eq!(local_id, new_local_id);
        let db_label = tx.label_with_id(local_id).unwrap().unwrap();
        assert_eq!(LocalLabel::from_label(local_id, None, label), db_label);
    });
}

#[test]
fn test_remote_label_update() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let mut labels = test_labels();
        tx.create_remote_labels(labels.iter()).unwrap();

        // Perform Some Updates
        labels[0].color = "#xxxxx".into();
        labels[0].name = "FooBar".into();
        labels[1].sticky = true;
        labels[1].expanded = true;
        labels[1].notify = true;
        labels[1].display = true;
        // Switch parents
        labels[2].parent_id = Some(labels[3].id.clone());
        labels[2].order = 3;
        labels[2].path = Some("Folder2/Folder1".to_string());
        labels[3].parent_id = None;
        labels[3].path = None;
        labels[3].order = 2;

        tx.update_remote_labels(labels.iter())
            .expect("failed to update labels");

        compare_remote_labels_with_local(&tx, labels.iter());
    });
}

#[test]
fn test_delete_remote() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let labels = test_labels();
        tx.create_remote_labels(labels.iter()).unwrap();

        tx.delete_remote_label(&labels[0].id)
            .expect("failed to delete local label");

        let remote_labels = labels
            .iter()
            .skip(1)
            .cloned()
            .map(|l| l.into())
            .collect::<Vec<_>>();

        assert_eq!(tx.labels().unwrap().len(), 12);

        compare_remote_labels_with_local(&tx, remote_labels.iter());
    });
}

#[test]
fn label_with_counts() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let label = Label {
            id: LabelId::from("label"),
            parent_id: None,
            name: "Label".to_owned(),
            path: None,
            color: "00".to_owned(),
            label_type: LabelType::Label,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        };

        let total_conv = 20u64;
        let unread_conv = 40u64;
        let total_msg = 200u64;
        let unread_msg = 600u64;

        let local_id = tx.create_remote_label(&label).unwrap();

        tx.create_or_update_conversation_counts(
            [ConversationCount {
                label_id: label.id.clone(),
                total: total_conv,
                unread: unread_conv,
            }]
            .iter(),
        )
        .unwrap();

        tx.create_or_update_message_counts(
            [MessageCount {
                label_id: label.id.clone(),
                total: total_msg,
                unread: unread_msg,
            }]
            .iter(),
        )
        .unwrap();

        let conv_count = tx
            .label_with_id_and_conversation_count(local_id)
            .unwrap()
            .unwrap();
        assert_eq!(conv_count.unread_count, unread_conv);
        assert_eq!(conv_count.total_count, total_conv);

        let msg_count = tx
            .label_with_id_and_message_count(local_id)
            .unwrap()
            .unwrap();
        assert_eq!(msg_count.unread_count, unread_msg);
        assert_eq!(msg_count.total_count, total_msg);
    });
}

#[test]
fn create_local_label() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        for t in [
            LabelType::Label,
            LabelType::Folder,
            LabelType::System,
            LabelType::ContactGroup,
        ] {
            let new_label = tx
                .create_label(
                    LabelType::Folder,
                    format!("Label-{:?}", t),
                    None,
                    None,
                    LabelColor::purple(),
                )
                .expect("failed to create label");
            let db_label = tx
                .label_with_id(new_label.id)
                .expect("failed to load label")
                .expect("should have a value");
            assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
        }
    });
}

#[test]
fn create_local_label_has_ascending_order_per_type() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        for t in [
            LabelType::Label,
            LabelType::Folder,
            LabelType::System,
            LabelType::ContactGroup,
        ] {
            let new_label1 = tx
                .create_label(
                    LabelType::Folder,
                    format!("Label-{:?}-01", t),
                    None,
                    None,
                    LabelColor::purple(),
                )
                .expect("failed to create label");
            let new_label2 = tx
                .create_label(
                    LabelType::Folder,
                    format!("Label-{:?}-02", t),
                    None,
                    None,
                    LabelColor::purple(),
                )
                .expect("failed to create label");
            assert_eq!(
                new_label1.order + 1,
                new_label2.order,
                "Label order for type {:?} does not match",
                t
            );
        }
    });
}

#[test]
fn update_local_label() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let new_label = tx
            .create_label(
                LabelType::Folder,
                "MyLabel".into(),
                None,
                None,
                LabelColor::purple(),
            )
            .expect("failed to create label");
        let new_label2 = tx
            .create_label(
                LabelType::Folder,
                "MyOtherLabel".into(),
                None,
                None,
                LabelColor::purple(),
            )
            .expect("failed to create label");

        fn compare_db_label(
            conn_ref: &MailSqliteConnectionImpl,
            id: LocalLabelId,
            f: impl FnOnce(&LocalLabel),
        ) {
            let db_label = conn_ref
                .label_with_id(id)
                .expect("failed to get label")
                .expect("must have value");
            (f)(&db_label);
        }

        tx.update_label_color(new_label.id, &LabelColor::black())
            .expect("failed to get label");
        compare_db_label(&tx, new_label.id, |l| {
            assert_eq!(l.color, LabelColor::black());
        });

        tx.update_label_name(new_label.id, "NewName")
            .expect("failed to get label");
        compare_db_label(&tx, new_label.id, |l| {
            assert_eq!(l.name, "NewName");
        });

        tx.update_label_parent(new_label.id, Some(new_label2.id), Some("MyLabel/NewName"))
            .expect("failed to get label");
        compare_db_label(&tx, new_label.id, |l| {
            assert_eq!(l.parent_id, Some(new_label2.id));
            assert_eq!(l.path, Some("MyLabel/NewName".into()));
        });
    });
}

#[test]
fn test_mark_labels_as_initialized() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let new_label = tx
            .create_label(
                LabelType::Folder,
                "MyLabel".into(),
                None,
                None,
                LabelColor::purple(),
            )
            .expect("failed to create label");
        assert!(!tx
            .check_if_label_is_initialized_conversations(new_label.id)
            .unwrap());
        tx.mark_label_as_initialized_conversations(new_label.id)
            .expect("failed to mark label as initialized");
        assert!(tx
            .check_if_label_is_initialized_conversations(new_label.id)
            .unwrap());
        assert!(!tx
            .check_if_label_is_initialized_messages(new_label.id)
            .unwrap());
        tx.mark_label_as_initialized_messages(new_label.id)
            .expect("failed to mark label as initialized");
        assert!(tx
            .check_if_label_is_initialized_messages(new_label.id)
            .unwrap());
    });
}

fn compare_remote_labels_with_local<'i>(
    conn_ref: &MailSqliteConnectionImpl,
    remote_labels: impl Iterator<Item = &'i Label>,
) {
    let local_labels = conn_ref.labels().unwrap();

    let find_label = |id: &LabelId| -> &LocalLabel {
        local_labels
            .iter()
            .find(|l| l.rid.as_ref() == Some(id))
            .expect("failed to find local label")
    };

    // Check if parent ids are correct.
    for remote_label in remote_labels {
        let local = find_label(&remote_label.id);
        compare_local_to_remote(&conn_ref, local, &remote_label);
    }
}

fn test_labels() -> Vec<Label> {
    vec![
        Label {
            id: LabelId::from("label_id"),
            parent_id: None,
            name: "MyLabel".to_string(),
            path: None,
            color: "#ffffff".to_string(),
            label_type: LabelType::Label,
            notify: false,
            display: true,
            sticky: false,
            expanded: true,
            order: 0,
        },
        Label {
            id: LabelId::from("50"),
            parent_id: None,
            name: "Inbox2".to_string(),
            path: None,
            color: "#ffffff".to_string(),
            label_type: LabelType::System,
            notify: true,
            display: false,
            sticky: true,
            expanded: false,
            order: 0,
        },
        Label {
            id: LabelId::from("Folder1"),
            parent_id: None,
            name: "Folder1".to_string(),
            path: None,
            color: "#ffffff".to_string(),
            label_type: LabelType::Folder,
            notify: true,
            display: true,
            sticky: false,
            expanded: false,
            order: 2,
        },
        Label {
            id: LabelId::from("Folder2"),
            parent_id: Some(LabelId::from("Folder1")),
            name: "Folder2".to_string(),
            path: Some("Folder1/Folder2".to_string()),
            color: "#ffffff".to_string(),
            label_type: LabelType::Folder,
            notify: false,
            display: false,
            sticky: true,
            expanded: true,
            order: 3,
        },
    ]
}

fn compare_local_to_remote(conn: &MailSqliteConnectionImpl, local: &LocalLabel, remote: &Label) {
    assert_eq!(
        local.rid.as_ref(),
        Some(&remote.id),
        "remote id does not match for {}",
        remote.id
    );
    assert_eq!(
        local.parent_id.is_some(),
        remote.parent_id.is_some(),
        "parent id state does not match for {}",
        remote.id
    );
    assert_eq!(
        local.name, remote.name,
        "name does not match for {}",
        remote.id
    );
    assert_eq!(
        local.path, remote.path,
        "path does not match for {}",
        remote.id
    );
    assert_eq!(
        local.color.as_ref(),
        remote.color,
        "color does not match for {}",
        remote.id
    );
    assert_eq!(
        local.label_type, remote.label_type,
        "label type does not match for {}",
        remote.id
    );
    assert_eq!(
        local.order, remote.order,
        "order does not match for {}",
        remote.id
    );
    let sticky: bool = remote.sticky.into();
    assert_eq!(
        local.sticky, sticky,
        "sticky does not match for {}",
        remote.id
    );

    let expanded: bool = remote.expanded.into();
    assert_eq!(
        local.expanded, expanded,
        "expanded does not match for {}",
        remote.id
    );
    let notify: bool = remote.notify.into();
    assert_eq!(
        local.notify, notify,
        "notified does not match for {}",
        remote.id
    );

    if let Some(local_parent_id) = local.parent_id {
        let parent_label = conn
            .label_with_id(local_parent_id)
            .expect("failed to find parent label")
            .expect("Parent label should exist");
        assert_eq!(
            parent_label.rid.unwrap(),
            *remote.parent_id.as_ref().unwrap(),
            "parent id value does not match for {}",
            remote.id
        );
    }
}
