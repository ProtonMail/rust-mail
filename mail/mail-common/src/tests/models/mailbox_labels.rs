use crate::models::MailboxLabels;
use proton_core_common::{
    datatypes::{LabelColor, LabelType},
    models::Label,
};
use proton_mail_common::test_utils::db::new_test_connection;
use stash::orm::Model;
use stash::stash::StashError;

#[tokio::test]
async fn test_mark_labels_as_initialized() {
    let mut tether = new_test_connection().await.connection().await.unwrap();
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
            let new_label_id = new_label.id();

            let mut mailbox_label = MailboxLabels::load(new_label_id, tx)
                .await
                .expect("failed to load label")
                .unwrap_or_else(|| MailboxLabels::new(new_label_id));

            // Newly created label is not initialized
            assert!(!mailbox_label.initialized);

            // Initializing
            mailbox_label.initialized = true;
            mailbox_label
                .save(tx)
                .await
                .expect("failed to mark label as initialized");

            // Load from the DB again
            let mailbox_label = MailboxLabels::load(new_label_id, tx)
                .await
                .expect("failed to load label")
                .unwrap_or_else(|| MailboxLabels::new(new_label_id));

            // Now it should be marked as initialized
            assert!(mailbox_label.initialized);
            Ok(())
        })
        .await
        .unwrap();
}
