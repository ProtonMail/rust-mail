use crate::{
    new_test_connection, with_tx, LabelColor, LocalLabel, LocalLabelId, MailSqliteConnectionImpl,
    RemoteLabel,
};
use proton_api_mail::domain::{Label, LabelId, LabelType};
use proton_api_mail::proton_api_core::domain::ProtonBoolean;

#[test]
fn test_remote_label_add() {
    let (mut conn, _, _guard) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let labels = test_labels();
        let remote_labels = labels.iter().cloned().map(|l| l.into()).collect::<Vec<_>>();
        tx.create_remote_labels(labels.iter()).unwrap();

        compare_remote_labels(&tx, &labels, &remote_labels);
        compare_remote_labels_with_local(&tx, remote_labels.iter());
    })
}

#[test]
fn test_remote_label_add_duplicate() {
    let (mut conn, _, _guard) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let label = Label {
            id: LabelId::from("label_id2"),
            parent_id: None,
            name: "MyLabel".to_string(),
            path: None,
            color: "#ffffff".to_string(),
            label_type: LabelType::Label,
            notify: ProtonBoolean::False,
            display: ProtonBoolean::True,
            sticky: ProtonBoolean::False,
            expanded: ProtonBoolean::True,
            order: 0,
        };

        let remote_label: [RemoteLabel; 1] = [label.clone().into()];
        tx.create_remote_label(&label).unwrap();
        tx.create_remote_label(&label).unwrap();

        compare_remote_labels(&tx, &[label], &remote_label);
        compare_remote_labels_with_local(&tx, remote_label.iter());
    });
}

#[test]
fn test_remote_label_update() {
    let (mut conn, _, _guard) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let mut labels = test_labels();
        tx.create_remote_labels(labels.iter()).unwrap();

        // Perform Some Updates
        labels[0].color = "#xxxxx".into();
        labels[0].name = "FooBar".into();
        labels[1].sticky = ProtonBoolean::True;
        labels[1].expanded = ProtonBoolean::True;
        labels[1].notify = ProtonBoolean::True;
        labels[1].display = ProtonBoolean::True;
        // Switch parents
        labels[2].parent_id = Some(labels[3].id.clone());
        labels[2].order = 3;
        labels[2].path = Some("Folder2/Folder1".to_string());
        labels[3].parent_id = None;
        labels[3].path = None;
        labels[3].order = 2;

        let remote_labels = labels.iter().cloned().map(|l| l.into()).collect::<Vec<_>>();

        tx.update_remote_labels(labels.iter())
            .expect("failed to update labels");

        compare_remote_labels_with_local(&tx, remote_labels.iter());
    });
}

#[test]
fn test_delete_remote() {
    let (mut conn, _, _guard) = new_test_connection();
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

        assert_eq!(tx.get_all_local_labels().unwrap().len(), 3);

        compare_remote_labels_with_local(&tx, remote_labels.iter());
    });
}

#[test]
fn create_local_label() {
    let (mut conn, _, _guard) = new_test_connection();
    with_tx(&mut conn, |tx| {
        for t in [
            LabelType::Label,
            LabelType::Folder,
            LabelType::System,
            LabelType::ContactGroup,
        ] {
            let new_label = tx
                .create_local_label(
                    LabelType::Folder,
                    format!("Label-{:?}", t),
                    None,
                    None,
                    LabelColor::purple(),
                )
                .expect("failed to create label");
            let db_label = tx
                .get_local_label(new_label.id)
                .expect("failed to load label")
                .expect("should have a value");
            assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
        }
    });
}

#[test]
fn create_local_label_has_ascending_order_per_type() {
    let (mut conn, _, _guard) = new_test_connection();
    with_tx(&mut conn, |tx| {
        for t in [
            LabelType::Label,
            LabelType::Folder,
            LabelType::System,
            LabelType::ContactGroup,
        ] {
            let new_label1 = tx
                .create_local_label(
                    LabelType::Folder,
                    format!("Label-{:?}-01", t),
                    None,
                    None,
                    LabelColor::purple(),
                )
                .expect("failed to create label");
            let new_label2 = tx
                .create_local_label(
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
    let (mut conn, _, _guard) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let new_label = tx
            .create_local_label(
                LabelType::Folder,
                "MyLabel".into(),
                None,
                None,
                LabelColor::purple(),
            )
            .expect("failed to create label");
        let new_label2 = tx
            .create_local_label(
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
                .get_local_label(id)
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

fn compare_remote_labels_with_local<'i>(
    conn_ref: &MailSqliteConnectionImpl,
    remote_labels: impl Iterator<Item = &'i RemoteLabel>,
) {
    let local_labels = conn_ref.get_all_local_labels().unwrap();

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

fn compare_remote_labels(
    conn_ref: &MailSqliteConnectionImpl,
    labels: &[Label],
    remote_labels: &[RemoteLabel],
) {
    let stored_labels = conn_ref
        .get_remote_labels(
            labels
                .iter()
                .map(|l| l.id.clone())
                .collect::<Vec<_>>()
                .iter(),
        )
        .unwrap();

    for remote_label in remote_labels {
        assert!(
            stored_labels.contains(remote_label),
            "label {:?} does not match",
            remote_label
        );
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
            notify: ProtonBoolean::False,
            display: ProtonBoolean::True,
            sticky: ProtonBoolean::False,
            expanded: ProtonBoolean::True,
            order: 0,
        },
        Label {
            id: LabelId::from("0"),
            parent_id: None,
            name: "Inbox".to_string(),
            path: None,
            color: "#ffffff".to_string(),
            label_type: LabelType::System,
            notify: ProtonBoolean::True,
            display: ProtonBoolean::False,
            sticky: ProtonBoolean::True,
            expanded: ProtonBoolean::False,
            order: 0,
        },
        Label {
            id: LabelId::from("Folder1"),
            parent_id: None,
            name: "Folder1".to_string(),
            path: None,
            color: "#ffffff".to_string(),
            label_type: LabelType::Folder,
            notify: ProtonBoolean::True,
            display: ProtonBoolean::True,
            sticky: ProtonBoolean::False,
            expanded: ProtonBoolean::False,
            order: 2,
        },
        Label {
            id: LabelId::from("Folder2"),
            parent_id: Some(LabelId::from("Folder1")),
            name: "Folder2".to_string(),
            path: Some("Folder1/Folder2".to_string()),
            color: "#ffffff".to_string(),
            label_type: LabelType::Folder,
            notify: ProtonBoolean::False,
            display: ProtonBoolean::False,
            sticky: ProtonBoolean::True,
            expanded: ProtonBoolean::True,
            order: 3,
        },
    ]
}

fn compare_local_to_remote(
    conn: &MailSqliteConnectionImpl,
    local: &LocalLabel,
    remote: &RemoteLabel,
) {
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
        local.color, remote.color,
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
    assert_eq!(
        local.sticky, remote.sticky,
        "sticky does not match for {}",
        remote.id
    );
    assert_eq!(
        local.expanded, remote.expanded,
        "expanded does not match for {}",
        remote.id
    );
    assert_eq!(
        local.notified, remote.notified,
        "notified does not match for {}",
        remote.id
    );

    if let Some(local_parent_id) = local.parent_id {
        let parent_label = conn
            .get_local_label(local_parent_id)
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
