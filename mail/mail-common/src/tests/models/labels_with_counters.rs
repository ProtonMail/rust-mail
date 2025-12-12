use crate::datatypes::ConversationLabelsCount;
use crate::datatypes::MessageLabelsCount;
use crate::models::LabelWithCounters;
use pretty_assertions::assert_eq;
use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType as ApiLabelType;
use proton_core_common::models::Label;
use proton_mail_common::test_utils::db::new_test_connection;
use stash::orm::Model;
use stash::stash::StashError;

#[tokio::test]
async fn label_with_counters() {
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let label = ApiLabel {
        id: LabelId::from("label"),
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

    let local_id = tether
        .tx::<_, _, StashError>(async |tx| {
            local_label.save(tx).await.unwrap();

            ConversationLabelsCount::create_or_update_conversation_counts(
                vec![ConversationLabelsCount {
                    label_id: local_label.remote_id.clone().unwrap(),
                    total: total_conv,
                    unread: unread_conv,
                }],
                tx,
            )
            .await
            .unwrap();

            MessageLabelsCount::create_or_update_message_counts(
                vec![MessageLabelsCount {
                    label_id: local_label.remote_id.clone().unwrap(),
                    total: total_msg,
                    unread: unread_msg,
                }],
                tx,
            )
            .await
            .unwrap();

            Ok(local_label.id())
        })
        .await
        .unwrap();

    let counters = LabelWithCounters::load(local_id, &tether)
        .await
        .expect("failed to load counter")
        .expect("should have a value");

    assert_eq!(counters.unread_conv, unread_conv);
    assert_eq!(counters.total_conv, total_conv);
    assert_eq!(counters.unread_msg, unread_msg);
    assert_eq!(counters.total_msg, total_msg);
}
