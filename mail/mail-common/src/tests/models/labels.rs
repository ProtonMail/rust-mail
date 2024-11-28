#![allow(clippy::module_inception)]
#![allow(non_snake_case)]

use super::*;
use crate::datatypes::{ConversationCount, LabelColor, LabelType, MessageCount};
use crate::models::Label;
use pretty_assertions::assert_eq;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_core_common::models::ModelExtension as _;
use proton_mail_test_utils::db::new_test_connection;
use proton_mail_test_utils::utils::random_string;
use stash::orm::Model;
use stash::params;
use stash::stash::{Stash, Tether};

#[tokio::test]
async fn test_remote_label_add() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    let labels = test_labels();
    for label in labels.clone() {
        Label::from(label).save_using(&tx).await.unwrap();
    }
    compare_remote_labels_with_local(&stash, labels).await;
}

#[tokio::test]
async fn test_remote_label_add_1_char_long_name() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    let label = test_label(random_string(1).as_str());

    Label::from(label.clone()).save_using(&tx).await.unwrap();
    compare_remote_label_with_local(&stash, label).await;
}

#[tokio::test]
async fn test_remote_label_add_100_char_long_name() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    let label = test_label(random_string(100).as_str());

    Label::from(label.clone()).save_using(&tx).await.unwrap();
    compare_remote_label_with_local(&stash, label).await;
}

#[tokio::test]
async fn test_remote_label_update() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    stash.execute("DELETE FROM labels", vec![]).await.unwrap();
    let mut labels = test_labels()
        .into_iter()
        .map(Label::from)
        .collect::<Vec<_>>();
    for label in &mut labels {
        label.save_using(&tx).await.unwrap();
    }

    let mut remote_labels = test_labels();
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
        label
            .save_using(&tx)
            .await
            .expect("failed to update labels");
    }

    compare_remote_labels_with_local(&stash, remote_labels).await;
}

#[tokio::test]
async fn test_delete_remote() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    let mut labels = test_labels();

    for label in labels.clone() {
        let mut label = Label::from(label);
        if let Some(parent_id) = label.remote_parent_id.clone() {
            label.local_parent_id = Label::find_by_id(RemoteId::from(parent_id), &stash)
                .await
                .expect("failed to get parent label")
                .expect("parent label should exist")
                .local_id;
        }
        label.save_using(&tx).await.unwrap();
    }

    tx.execute(
        "DELETE FROM labels WHERE remote_id = ?",
        params![LabelId::from(labels[0].id.clone())],
    )
    .await
    .expect("failed to delete local label");

    labels.remove(0);

    let remote_labels = labels;

    compare_remote_labels_with_local(&stash, remote_labels).await;
}

#[tokio::test]
async fn label_with_counts() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    let label = ApiLabel {
        id: ApiRemoteId::from("label"),
        parent_id: None,
        name: "Label".to_owned(),
        path: None,
        color: "00".to_owned(),
        label_type: ApiLabelType::Label,
        notify: false,
        display: false,
        sticky: false,
        expanded: false,
        order: 0,
    };

    let total_conv = 20_u64;
    let unread_conv = 40_u64;
    let total_msg = 200_u64;
    let unread_msg = 600_u64;

    let mut local_label = Label::from(label.clone());
    local_label.save_using(&tx).await.unwrap();
    let local_id = local_label.local_id.unwrap();

    Label::create_or_update_conversation_counts(
        vec![ConversationCount {
            label_id: local_label.remote_id.clone().unwrap(),
            total: total_conv,
            unread: unread_conv,
        }],
        tx.stash(),
    )
    .await
    .unwrap();

    Label::create_or_update_message_counts(
        vec![MessageCount {
            label_id: local_label.remote_id.clone().unwrap(),
            total: total_msg,
            unread: unread_msg,
        }],
        tx.stash(),
    )
    .await
    .unwrap();

    let label = Label::load(local_id, &tx)
        .await
        .expect("failed to load label")
        .expect("should have a value");
    assert_eq!(label.unread_conv, unread_conv);
    assert_eq!(label.total_conv, total_conv);

    assert_eq!(label.unread_msg, unread_msg);
    assert_eq!(label.total_msg, total_msg);
}

#[tokio::test]
async fn create_local_label() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    for t in [
        LabelType::Label,
        LabelType::Folder,
        LabelType::System,
        LabelType::ContactGroup,
    ] {
        let mut new_label = Label {
            local_id: None,
            remote_id: Some(format!("Label-{:?}", t).into()),
            local_parent_id: None,
            remote_parent_id: None,
            color: LabelColor::purple(),
            display: false,
            display_order: 0,
            expanded: false,
            initialized_conv: false,
            initialized_msg: false,
            label_type: LabelType::Folder,
            name: "Label".to_owned(),
            notify: false,
            path: None,
            sticky: false,
            total_conv: 0,
            total_msg: 0,
            unread_conv: 0,
            unread_msg: 0,
            row_id: None,
            stash: Some(stash.clone()),
        };
        new_label
            .save_using(&tx)
            .await
            .expect("failed to create label");
        let db_label = Label::load(new_label.local_id.unwrap(), &tx)
            .await
            .expect("failed to load label")
            .expect("should have a value");
        assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
    }
}

#[tokio::test]
async fn create_local_label_1_char_long_name() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    for t in [LabelType::Label, LabelType::Folder] {
        let label_name = random_string(1);
        let mut new_label = Label {
            local_id: None,
            remote_id: Some(format!("Label-{:?}", t).into()),
            local_parent_id: None,
            remote_parent_id: None,
            color: LabelColor::purple(),
            display: false,
            display_order: 0,
            expanded: false,
            initialized_conv: false,
            initialized_msg: false,
            label_type: LabelType::Folder,
            name: label_name.to_owned(),
            notify: false,
            path: None,
            sticky: false,
            total_conv: 0,
            total_msg: 0,
            unread_conv: 0,
            unread_msg: 0,
            row_id: None,
            stash: Some(stash.clone()),
        };
        new_label
            .save_using(&tx)
            .await
            .expect("failed to create label");
        let db_label = Label::load(new_label.local_id.unwrap(), &tx)
            .await
            .expect("failed to load label")
            .expect("should have a value");
        assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
    }
}

#[tokio::test]
async fn create_local_label_100_char_long_name() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    for t in [LabelType::Label, LabelType::Folder] {
        let label_name = random_string(100);
        let mut new_label = Label {
            local_id: None,
            remote_id: Some(format!("Label-{:?}", t).into()),
            local_parent_id: None,
            remote_parent_id: None,
            color: LabelColor::purple(),
            display: false,
            display_order: 0,
            expanded: false,
            initialized_conv: false,
            initialized_msg: false,
            label_type: LabelType::Folder,
            name: label_name.to_owned(),
            notify: false,
            path: None,
            sticky: false,
            total_conv: 0,
            total_msg: 0,
            unread_conv: 0,
            unread_msg: 0,
            row_id: None,
            stash: Some(stash.clone()),
        };
        new_label
            .save_using(&tx)
            .await
            .expect("failed to create label");
        let db_label = Label::load(new_label.local_id.unwrap(), &tx)
            .await
            .expect("failed to load label")
            .expect("should have a value");
        assert_eq!(new_label, db_label, "Label of type {:?} does not match", t);
    }
}

#[tokio::test]
async fn create_local_label_has_ascending_order_per_type() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    for t in [
        LabelType::Label,
        LabelType::Folder,
        LabelType::System,
        LabelType::ContactGroup,
    ] {
        let mut new_label1 = Label {
            local_id: None,
            remote_id: Some(format!("Label-{:?}-01", t).into()),
            local_parent_id: None,
            remote_parent_id: None,
            color: LabelColor::purple(),
            display: false,
            display_order: 0,
            expanded: false,
            initialized_conv: false,
            initialized_msg: false,
            label_type: LabelType::Folder,
            name: "Label".to_owned(),
            notify: false,
            path: None,
            sticky: false,
            total_conv: 0,
            total_msg: 0,
            unread_conv: 0,
            unread_msg: 0,
            row_id: None,
            stash: None,
        };
        new_label1
            .save_using(&tx)
            .await
            .expect("failed to create label");
        let mut new_label2 = Label {
            local_id: None,
            remote_id: Some(format!("Label-{:?}-02", t).into()),
            local_parent_id: None,
            remote_parent_id: None,
            color: LabelColor::purple(),
            display: false,
            display_order: 0,
            expanded: false,
            initialized_conv: false,
            initialized_msg: false,
            label_type: LabelType::Folder,
            name: "Label".to_owned(),
            notify: false,
            path: None,
            sticky: false,
            total_conv: 0,
            total_msg: 0,
            unread_conv: 0,
            unread_msg: 0,
            row_id: None,
            stash: None,
        };
        new_label2
            .save_using(&tx)
            .await
            .expect("failed to create label");
        // TODO
        // assert_eq!(
        //     new_label1.display_order + 1,
        //     new_label2.display_order,
        //     "Label order for type {:?} does not match",
        //     t
        // );
    }
}

#[tokio::test]
async fn update_local_label() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    let mut new_label = Label {
        local_id: None,
        remote_id: Some("MyLabel".into()),
        local_parent_id: None,
        remote_parent_id: None,
        color: LabelColor::purple(),
        display: false,
        display_order: 0,
        expanded: false,
        initialized_conv: false,
        initialized_msg: false,
        label_type: LabelType::Folder,
        name: "Label".to_owned(),
        notify: false,
        path: None,
        sticky: false,
        total_conv: 0,
        total_msg: 0,
        unread_conv: 0,
        unread_msg: 0,
        row_id: None,
        stash: None,
    };
    new_label
        .save_using(&tx)
        .await
        .expect("failed to create label");
    let new_label2 = Label {
        local_id: None,
        remote_id: Some("MyOtherLabel".into()),
        local_parent_id: None,
        remote_parent_id: None,
        color: LabelColor::purple(),
        display: false,
        display_order: 0,
        expanded: false,
        initialized_conv: false,
        initialized_msg: false,
        label_type: LabelType::Folder,
        name: "Label".to_owned(),
        notify: false,
        path: None,
        sticky: false,
        total_conv: 0,
        total_msg: 0,
        unread_conv: 0,
        unread_msg: 0,
        row_id: None,
        stash: None,
    };
    new_label
        .save_using(&tx)
        .await
        .expect("failed to create label");

    async fn compare_db_label(tx: &Tether, id: LocalId, f: impl FnOnce(&Label)) {
        let db_label = Label::load(id, tx)
            .await
            .expect("failed to get label")
            .expect("must have value");
        (f)(&db_label);
    }

    new_label.color = LabelColor::black();
    new_label
        .save_using(&tx)
        .await
        .expect("failed to save label");
    compare_db_label(&tx, new_label.local_id.unwrap(), |l| {
        assert_eq!(l.color, LabelColor::black());
    })
    .await;

    new_label.name = "NewName".to_owned();
    new_label
        .save_using(&tx)
        .await
        .expect("failed to save label");
    compare_db_label(&tx, new_label.local_id.unwrap(), |l| {
        assert_eq!(l.name, "NewName");
    })
    .await;

    new_label.remote_parent_id = new_label2.remote_id.clone();
    new_label.path = Some("MyLabel/NewName".into());
    new_label
        .save_using(&tx)
        .await
        .expect("failed to save label");
    compare_db_label(&tx, new_label.local_id.unwrap(), |l| {
        assert_eq!(l.remote_parent_id, new_label2.remote_id);
        assert_eq!(l.path, Some("MyLabel/NewName".into()));
    })
    .await;
}

#[tokio::test]
async fn test_mark_labels_as_initialized() {
    let stash = new_test_connection().await;
    let tx = stash.connection();
    let mut new_label = Label {
        local_id: None,
        remote_id: Some("MyLabel".into()),
        local_parent_id: None,
        remote_parent_id: None,
        color: LabelColor::purple(),
        display: false,
        display_order: 0,
        expanded: false,
        initialized_conv: false,
        initialized_msg: false,
        label_type: LabelType::Folder,
        name: "Label".to_owned(),
        notify: false,
        path: None,
        sticky: false,
        total_conv: 0,
        total_msg: 0,
        unread_conv: 0,
        unread_msg: 0,
        row_id: None,
        stash: None,
    };
    new_label
        .save_using(&tx)
        .await
        .expect("failed to create label");
    assert!(!new_label.initialized_conv);
    new_label.initialized_conv = true;
    new_label
        .save_using(&tx)
        .await
        .expect("failed to mark label as initialized");
    assert!(new_label.initialized_conv);
    assert!(!new_label.initialized_msg);
    new_label.initialized_msg = true;
    new_label
        .save_using(&tx)
        .await
        .expect("failed to mark label as initialized");
    assert!(new_label.initialized_msg);
}

#[tokio::test]
async fn test_watch_label() {
    let stash = new_test_connection().await;
    // let tx = stash.connection();

    let mut label: Label = ApiLabel {
        id: ApiRemoteId::from("label_id"),
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

    label.set_stash(&stash);
    label.save_using(&stash).await.unwrap();

    let (db_label, watcher) = Label::watch(label.local_id.unwrap(), &stash)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(db_label, label);

    label.display_order = 10;
    label.save_using(&stash).await.unwrap();

    watcher.recv_async().await.unwrap();
}

async fn compare_remote_labels_with_local(stash: &Stash, remote_labels: Vec<ApiLabel>) {
    for remote_label in remote_labels {
        compare_remote_label_with_local(stash, remote_label).await;
    }
}

async fn compare_remote_label_with_local(stash: &Stash, remote_label: ApiLabel) {
    let local_labels = Label::all(stash, None).await.expect("failed to get labels");
    let find_label = |id: &LabelId| -> &Label {
        local_labels
            .iter()
            .find(|l| l.remote_id == Some(id.clone()))
            .expect("failed to find local label")
    };

    // Check if parent ids are correct.
    let local = find_label(&LabelId::from(remote_label.id.clone()));
    compare_local_to_remote(stash, local, &remote_label).await;
}

fn test_labels() -> Vec<ApiLabel> {
    vec![
        ApiLabel {
            id: ApiRemoteId::from("label_id"),
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
            id: ApiRemoteId::from("50"),
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
            id: ApiRemoteId::from("Folder1"),
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
            id: ApiRemoteId::from("Folder2"),
            parent_id: Some(ApiRemoteId::from("Folder1")),
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
        id: ApiRemoteId::from("label_id"),
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

async fn compare_local_to_remote(stash: &Stash, local: &Label, remote: &ApiLabel) {
    assert_eq!(
        local.remote_id,
        Some(remote.id.clone().into()),
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
        let parent_label = Label::find_by_id(RemoteId::from(remote_parent_id), stash)
            .await
            .expect("failed to find parent label")
            .expect("Parent label should exist");
        assert_eq!(
            parent_label.remote_id.unwrap(),
            remote.parent_id.clone().unwrap().into(),
            "parent id value does not match for {}",
            remote.id
        );
    }
}
