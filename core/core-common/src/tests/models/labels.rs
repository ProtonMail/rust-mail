use super::*;
use crate::models::Label;
use crate::models::ModelExtension as _;
use crate::tests::common::new_core_test_connection;
use pretty_assertions::assert_eq;
use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType as ApiLabelType;
use proton_core_common::test_utils::utils::random_string;
use stash::orm::Model;
use stash::params;
use stash::stash::Tether;

#[tokio::test]
async fn test_remote_label_add() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    let labels = test_labels();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for label in labels.clone() {
                Label::from(label).save(tx).await?;
            }
            Ok(())
        })
        .await
        .unwrap();
    compare_remote_labels_with_local(&tether, labels).await;
}

#[tokio::test]
async fn test_remote_label_add_1_char_long_name() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    let label = test_label(random_string(1).as_str());
    tether
        .tx::<_, _, StashError>(async |tx| Label::from(label.clone()).save(tx).await)
        .await
        .unwrap();
    compare_remote_label_with_local(&tether, label).await;
}

#[tokio::test]
async fn test_remote_label_add_100_char_long_name() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    let label = test_label(random_string(100).as_str());
    tether
        .tx(async |tx| Label::from(label.clone()).save(tx).await)
        .await
        .unwrap();
    compare_remote_label_with_local(&tether, label).await;
}

#[tokio::test]
async fn test_remote_label_update() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether.execute("DELETE FROM labels", vec![]).await.unwrap();
    let mut labels = test_labels()
        .into_iter()
        .map(Label::from)
        .collect::<Vec<_>>();
    let mut remote_labels = test_labels();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for label in &mut labels {
                label.save(tx).await.unwrap();
            }

            // Perform Some Updates
            remote_labels[0].color = "#xxxxx".into();
            remote_labels[0].name = "FooBar".into();
            remote_labels[1].sticky = true;
            remote_labels[1].expanded = true;
            remote_labels[1].notify = true;
            remote_labels[1].display = true;
            // Switch parents
            remote_labels[2].parent_id = Some(remote_labels[3].id.clone());
            remote_labels[2].order = 3;
            remote_labels[2].path = Some("Folder2/Folder1".to_owned());
            remote_labels[3].parent_id = None;
            remote_labels[3].path = None;
            remote_labels[3].order = 2;

            // Perform Some Updates
            labels[0].color = "#xxxxx".into();
            labels[0].name = "FooBar".into();
            labels[1].sticky = true;
            labels[1].expanded = true;
            labels[1].notify = true;
            labels[1].display = true;
            // Switch parents
            labels[2].remote_parent_id = labels[3].remote_id.clone();
            labels[2].display_order = 3;
            labels[2].path = Some("Folder2/Folder1".to_owned());
            labels[3].remote_parent_id = None;
            labels[3].path = None;
            labels[3].display_order = 2;

            for label in &mut labels {
                label.save(tx).await?;
            }
            Ok(())
        })
        .await
        .unwrap();

    compare_remote_labels_with_local(&tether, remote_labels).await;
}

#[tokio::test]
async fn test_delete_remote() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    let mut labels = test_labels();

    tether
        .tx::<_, _, StashError>(async |tx| {
            for label in labels.clone() {
                let mut label = Label::from(label);
                if let Some(parent_id) = label.remote_parent_id.clone() {
                    label.local_parent_id = Label::find_by_remote_id(parent_id, tx)
                        .await
                        .expect("failed to get parent label")
                        .expect("parent label should exist")
                        .local_id;
                }
                label.save(tx).await.unwrap();
            }

            tx.execute(
                "DELETE FROM labels WHERE remote_id = ?",
                params![labels[0].id.clone()],
            )
            .await
            .expect("failed to delete local label");
            Ok(())
        })
        .await
        .unwrap();

    labels.remove(0);

    let remote_labels = labels;

    compare_remote_labels_with_local(&tether, remote_labels).await;
}

#[tokio::test]
async fn create_local_label() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for t in [
                LabelType::Label,
                LabelType::Folder,
                LabelType::System,
                LabelType::ContactGroup,
            ] {
                let mut new_label = Label {
                    local_id: None,
                    remote_id: Some(format!("Label-{t:?}").into()),
                    local_parent_id: None,
                    remote_parent_id: None,
                    color: LabelColor::purple(),
                    display: false,
                    display_order: 0,
                    expanded: false,
                    label_type: LabelType::Folder,
                    name: "Label".to_owned(),
                    notify: false,
                    path: None,
                    sticky: false,
                };
                new_label.save(tx).await.expect("failed to create label");
                let db_label = Label::load(new_label.id(), tx)
                    .await
                    .expect("failed to load label")
                    .expect("should have a value");
                assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
            }
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn create_local_label_1_char_long_name() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for t in [LabelType::Label, LabelType::Folder] {
                let label_name = random_string(1);
                let mut new_label = Label {
                    local_id: None,
                    remote_id: Some(format!("Label-{t:?}").into()),
                    local_parent_id: None,
                    remote_parent_id: None,
                    color: LabelColor::purple(),
                    display: false,
                    display_order: 0,
                    expanded: false,
                    label_type: LabelType::Folder,
                    name: label_name.clone(),
                    notify: false,
                    path: None,
                    sticky: false,
                };
                new_label.save(tx).await.expect("failed to create label");
                let db_label = Label::load(new_label.id(), tx)
                    .await
                    .expect("failed to load label")
                    .expect("should have a value");
                assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
            }
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn create_local_label_100_char_long_name() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for t in [LabelType::Label, LabelType::Folder] {
                let label_name = random_string(100);
                let mut new_label = Label {
                    local_id: None,
                    remote_id: Some(format!("Label-{t:?}").into()),
                    local_parent_id: None,
                    remote_parent_id: None,
                    color: LabelColor::purple(),
                    display: false,
                    display_order: 0,
                    expanded: false,
                    label_type: LabelType::Folder,
                    name: label_name.clone(),
                    notify: false,
                    path: None,
                    sticky: false,
                };
                new_label.save(tx).await.expect("failed to create label");
                let db_label = Label::load(new_label.id(), tx)
                    .await
                    .expect("failed to load label")
                    .expect("should have a value");
                assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
            }
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn create_local_label_has_ascending_order_per_type() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            for t in [
                LabelType::Label,
                LabelType::Folder,
                LabelType::System,
                LabelType::ContactGroup,
            ] {
                let mut new_label1 = Label {
                    local_id: None,
                    remote_id: Some(format!("Label-{t:?}-01").into()),
                    local_parent_id: None,
                    remote_parent_id: None,
                    color: LabelColor::purple(),
                    display: false,
                    display_order: 0,
                    expanded: false,
                    label_type: LabelType::Folder,
                    name: "Label".to_owned(),
                    notify: false,
                    path: None,
                    sticky: false,
                };
                new_label1.save(tx).await.expect("failed to create label");
                let mut new_label2 = Label {
                    local_id: None,
                    remote_id: Some(format!("Label-{t:?}-02").into()),
                    local_parent_id: None,
                    remote_parent_id: None,
                    color: LabelColor::purple(),
                    display: false,
                    display_order: 0,
                    expanded: false,
                    label_type: LabelType::Folder,
                    name: "Label".to_owned(),
                    notify: false,
                    path: None,
                    sticky: false,
                };
                new_label2.save(tx).await.expect("failed to create label");
                // TODO
                // assert_eq!(
                //     new_label1.display_order + 1,
                //     new_label2.display_order,
                //     "Label order for type {:?} does not match",
                //     t
                // );
            }
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn update_local_label() {
    let mut tether = new_core_test_connection().await.connection().await.unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut new_label = Label {
                local_id: None,
                remote_id: Some("MyLabel".into()),
                local_parent_id: None,
                remote_parent_id: None,
                color: LabelColor::purple(),
                display: false,
                display_order: 0,
                expanded: false,
                label_type: LabelType::Folder,
                name: "Label".to_owned(),
                notify: false,
                path: None,
                sticky: false,
            };
            new_label.save(tx).await.expect("failed to create label");
            let new_label2 = Label {
                local_id: None,
                remote_id: Some("MyOtherLabel".into()),
                local_parent_id: None,
                remote_parent_id: None,
                color: LabelColor::purple(),
                display: false,
                display_order: 0,
                expanded: false,
                label_type: LabelType::Folder,
                name: "Label".to_owned(),
                notify: false,
                path: None,
                sticky: false,
            };
            new_label.save(tx).await.expect("failed to create label");

            new_label.color = LabelColor::black();
            new_label.save(tx).await.expect("failed to save label");
            compare_db_label(tx, new_label.id(), |l| {
                assert_eq!(l.color, LabelColor::black());
            })
            .await;

            new_label.name = "NewName".to_owned();
            new_label.save(tx).await.expect("failed to save label");

            compare_db_label(tx, new_label.id(), |l| {
                assert_eq!(l.name, "NewName");
            })
            .await;

            new_label.remote_parent_id = new_label2.remote_id.clone();
            new_label.path = Some("MyLabel/NewName".into());
            new_label.save(tx).await.expect("failed to save label");

            compare_db_label(tx, new_label.id(), |l| {
                assert_eq!(l.remote_parent_id, new_label2.remote_id);
                assert_eq!(l.path, Some("MyLabel/NewName".into()));
            })
            .await;
            Ok(())
        })
        .await
        .unwrap();
}

async fn compare_db_label(tx: &Tether, id: LocalLabelId, f: impl FnOnce(&Label)) {
    let db_label = Label::load(id, tx)
        .await
        .expect("failed to get label")
        .expect("must have value");
    (f)(&db_label);
}

#[tokio::test]
async fn test_watch_label() {
    let stash = new_core_test_connection().await;
    let mut tether = stash.connection().await.unwrap();
    let mut label = tether
        .tx::<_, _, StashError>(async |tx| {
            let mut label: Label = ApiLabel {
                id: LabelId::from("label_id"),
                parent_id: None,
                name: "MyLabel".to_owned(),
                path: None,
                color: "#ffffff".to_owned(),
                label_type: ApiLabelType::Label,
                notify: false,
                display: true,
                sticky: false,
                expanded: true,
                order: 0,
            }
            .into();

            label.save(tx).await.unwrap();
            Ok(label)
        })
        .await
        .unwrap();

    let db_label = Label::load(label.id(), &tether).await.unwrap().unwrap();
    let handle = Label::watch(&stash).unwrap();
    let watcher = &handle.receiver;

    assert_eq!(db_label, label);

    label.display_order = 10;
    tether
        .tx::<_, _, StashError>(async |tx| label.save(tx).await)
        .await
        .unwrap();

    watcher.recv_async().await.unwrap();
}

async fn compare_remote_labels_with_local(tether: &Tether, remote_labels: Vec<ApiLabel>) {
    for remote_label in remote_labels {
        compare_remote_label_with_local(tether, remote_label).await;
    }
}

async fn compare_remote_label_with_local(tether: &Tether, remote_label: ApiLabel) {
    let local_labels = Label::all(tether).await.expect("failed to get labels");
    let find_label = |id: &LabelId| -> &Label {
        local_labels
            .iter()
            .find(|l| l.remote_id == Some(id.clone()))
            .expect("failed to find local label")
    };

    // Check if parent ids are correct.
    let local = find_label(&remote_label.id);
    compare_local_to_remote(tether, local, &remote_label).await;
}

fn test_labels() -> Vec<ApiLabel> {
    vec![
        ApiLabel {
            id: LabelId::from("label_id"),
            parent_id: None,
            name: "MyLabel".to_owned(),
            path: None,
            color: "#ffffff".to_owned(),
            label_type: ApiLabelType::Label,
            notify: false,
            display: true,
            sticky: false,
            expanded: true,
            order: 0,
        },
        ApiLabel {
            id: LabelId::from("50"),
            parent_id: None,
            name: "Inbox2".to_owned(),
            path: None,
            color: "#ffffff".to_owned(),
            label_type: ApiLabelType::System,
            notify: true,
            display: false,
            sticky: true,
            expanded: false,
            order: 0,
        },
        ApiLabel {
            id: LabelId::from("Folder1"),
            parent_id: None,
            name: "Folder1".to_owned(),
            path: None,
            color: "#ffffff".to_owned(),
            label_type: ApiLabelType::Folder,
            notify: true,
            display: true,
            sticky: false,
            expanded: false,
            order: 2,
        },
        ApiLabel {
            id: LabelId::from("Folder2"),
            parent_id: Some(LabelId::from("Folder1")),
            name: "Folder2".to_owned(),
            path: Some("Folder1/Folder2".to_owned()),
            color: "#ffffff".to_owned(),
            label_type: ApiLabelType::Folder,
            notify: false,
            display: false,
            sticky: true,
            expanded: true,
            order: 3,
        },
    ]
}

fn test_label(name: &str) -> ApiLabel {
    ApiLabel {
        id: LabelId::from("label_id"),
        parent_id: None,
        name: name.to_owned(),
        path: None,
        color: "#ffffff".to_owned(),
        label_type: ApiLabelType::Label,
        notify: false,
        display: true,
        sticky: false,
        expanded: true,
        order: 0,
    }
}

async fn compare_local_to_remote(tether: &Tether, local: &Label, remote: &ApiLabel) {
    assert_eq!(
        local.remote_id,
        Some(remote.id.clone()),
        "remote id does not match for {}",
        remote.id
    );
    assert_eq!(
        local.remote_parent_id.is_some(),
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
        local.color.to_string(),
        remote.color,
        "color does not match for {}",
        remote.id
    );
    assert_eq!(
        local.label_type,
        remote.label_type.into(),
        "label type does not match for {}",
        remote.id
    );
    assert_eq!(
        local.display_order, remote.order,
        "order does not match for {}",
        remote.id
    );
    let sticky: bool = remote.sticky;
    assert_eq!(
        local.sticky, sticky,
        "sticky does not match for {}",
        remote.id
    );

    let expanded: bool = remote.expanded;
    assert_eq!(
        local.expanded, expanded,
        "expanded does not match for {}",
        remote.id
    );
    let notify: bool = remote.notify;
    assert_eq!(
        local.notify, notify,
        "notified does not match for {}",
        remote.id
    );

    if let Some(remote_parent_id) = local.remote_parent_id.clone() {
        let parent_label = Label::find_by_remote_id(remote_parent_id, tether)
            .await
            .expect("failed to find parent label")
            .expect("Parent label should exist");
        assert_eq!(
            parent_label.remote_id.unwrap(),
            remote.parent_id.clone().unwrap(),
            "parent id value does not match for {}",
            remote.id
        );
    }
}
