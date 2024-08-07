#![allow(non_snake_case)]

use super::super::*;
use crate::datatypes::{
    ConversationCount, LabelColor, LabelType, MessageAddress, MessageFlags, SystemLabelId,
};
use crate::db::new_test_connection_file;
use crate::tests::common::{
    create_address, create_labels, test_conversation, test_starred_label, MY_ATTACHMENT_ID,
    MY_LABEL_ID1, MY_LABEL_ID2,
};
use crate::tests::db_states::{
    new_test_delete_db_state, new_test_label_db_state,
    new_test_label_db_state_label_with_existing_labels, new_test_unread_db_state,
};
use crate::tests::utils::{
    conv_counts_as_map, message_counts_for_conversation, msg_counts_as_map,
    prepare_and_patch_db_state, prepare_and_patch_db_state_and_skip, prepare_db_state_core,
};
use lazy_static::lazy_static;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, ConversationLabel as ApiConversationLabel,
    Disposition as ApiDisposition, MimeType as ApiMimeType,
};
use proton_core_common::datatypes::LabelId;
use stash::orm::Model;
use stash::params;
use test_case::test_case;

lazy_static! {
    static ref STARRED: Label = new_label(LabelType::System, Some(LabelId::starred().clone()));
    static ref LABEL: Label = new_label(LabelType::Label, None);
    static ref FOLDER: Label = new_label(LabelType::Folder, None);
    static ref INBOX: Label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    static ref DRAFTS: Label = new_label(LabelType::System, Some(LabelId::drafts().clone())); // There is no conversations in drafts - this is theoretical case
    static ref ALL_LABELS: Vec<&'static Label> =
        vec![&STARRED, &LABEL, &FOLDER, &INBOX, &DRAFTS];
    static ref MOVED_CONV_LABELS: Vec<&'static Label> =
        vec![&STARRED, &LABEL, &FOLDER];
    static ref INBOX_AND_DRAFTS_LABELS: Vec<&'static Label> = vec![&INBOX, &DRAFTS];
}

#[test_case(
    &ALL_LABELS, &[], None; "TEST1 - empty messages"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED, false),], Some(0); "TEST2 - read - recieved message"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::empty(), false),], None; "TEST3 - read - draft message"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::OPENED, false),], None; "TEST4 - read - draft & opened message"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::OPENED, true),], None; "TEST5 - unread - draft & opened message"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED | MessageFlags::OPENED, true),], Some(0); "TEST6 - unread - recieved & opened message"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED, true),], Some(0); "TEST7 - unread - recieved message"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED | MessageFlags::INTERNAL, true),], Some(0); "TEST8 - unread - recieved & internal message"
)]
#[test_case(
    &ALL_LABELS, &[(MessageFlags::SENT | MessageFlags::INTERNAL, true),], Some(0); "TEST9 - unread - opened & internal message"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED | MessageFlags::INTERNAL | MessageFlags::OPENED, true),
        (MessageFlags::RECEIVED | MessageFlags::INTERNAL, true),

    ], Some(2); "TEST10 - all unread - recieved | internal | opened messages"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::empty(), true),

    ], Some(0); "TEST11 - all unread - recieved | draft messages"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::empty(), false),

    ], Some(0); "TEST12 - some unread - recieved | draft messages"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::SENT, true),
        (MessageFlags::SENT, true),
        (MessageFlags::empty(), false),

    ], Some(0); "TEST13 - some unread - sent | draft messages"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::SENT | MessageFlags::RECEIVED, true),
        (MessageFlags::SENT | MessageFlags::RECEIVED, true),
        (MessageFlags::empty(), false),

    ], Some(0); "TEST14 - some unread - sent & received | draft messages"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::empty(), true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::empty(), true),

    ], Some(3); "TEST15 - all unread - received | draft messages"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
    ], Some(2); "TEST16 - first_unread_conversation_message_in_starred_or_custom_label_or_folder"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::empty(), true),
        (MessageFlags::RECEIVED, true),
    ], Some(3); "TEST17 - first_unread_conversation_message_in_starred_or_custom_label_or_folder_non_consecutive_with_draft"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::empty(), true),
    ], Some(2); "TEST18 - first_unread_conversation_message_in_starred_or_custom_label_or_folder_non_consecutive_with_draft"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::empty(), true),
    ], Some(0); "TEST19 - first_unread_conversation_message_in_starred_or_custom_label_or_folder_non_consecutive_with_draft"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
    ], Some(2); "TEST20 - first_unread_conversation_message_default_last_consecutive_unread"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::empty(), true),
    ], Some(2); "TEST21 - first_unread_conversation_message_default_last_consecutive_unread_if_last_is_draft_or_auto_send"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::SENT | MessageFlags::AUTO, true),
    ], Some(2); "TEST22 - first_unread_conversation_message_default_last_consecutive_unread_if_last_is_draft_or_auto_send"
)]
#[test_case(
    &MOVED_CONV_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::SENT | MessageFlags::AUTO, true),
        (MessageFlags::empty(), true),
        (MessageFlags::RECEIVED, false),
    ], Some(2); "TEST23A - first_unread_conversation_message_default_last_nonconsecutive_not_draft_or_auto_send"
)]
#[test_case(
    &INBOX_AND_DRAFTS_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::SENT | MessageFlags::AUTO, true),
        (MessageFlags::empty(), true),
        (MessageFlags::RECEIVED, false),
    ], Some(0); "TEST23B - first_unread_conversation_message_default_last_nonconsecutive_not_draft_or_auto_send"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
        (MessageFlags::RECEIVED, true),
    ], Some(0); "TEST24 - oldest_unread_message_selected_in_unread_chain"
)]
#[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED, false),
        (MessageFlags::RECEIVED, false),
    ], Some(2); "TEST25 - all read"
)]
fn find_conversation_message_id(
    labels: &[&Label],
    messages: &[(MessageFlags, bool)],
    expected_id: Option<u64>,
) {
    let messages = messages
        .iter()
        .enumerate()
        .map(|(id, (flags, unread))| message_metadata_with_flags(id as u64, *flags, *unread))
        .collect::<Vec<_>>();

    for label in labels {
        assert_eq!(
            Conversation::first_unread_message(&label, &messages),
            expected_id,
            "Test failed for label: {:?}, {:?}",
            label.label_type,
            label.remote_id
        );
    }
}

fn message_metadata_with_flags(id: u64, flags: MessageFlags, unread: bool) -> Message {
    Message {
        local_id: Some(id),
        unread,
        sender: MessageAddress {
            address: String::new(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: String::new(),
        },
        flags,
        ..Default::default()
    }
}

fn new_label(label_type: LabelType, rid: Option<LabelId>) -> Label {
    Label {
        local_id: None,
        remote_id: rid,
        local_parent_id: None,
        remote_parent_id: None,
        color: LabelColor::black(),
        display: false,
        display_order: 0,
        expanded: false,
        initialized_conv: false,
        initialized_msg: false,
        label_type,
        name: String::new(),
        notify: false,
        path: None,
        sticky: false,
        total_conv: 0,
        total_msg: 0,
        unread_conv: 0,
        unread_msg: 0,
        row_id: None,
        stash: None,
    }
}

#[tokio::test]
async fn test_conversation_create_no_labels() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    create_labels(&tx).await;
    let conv = test_conversation(vec![], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.stash = Some(stash.clone());
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation, local_conversation);
}

#[tokio::test]
async fn test_conversation_has_messages_flag() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    create_labels(&tx).await;
    let conv = test_conversation(vec![], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.stash = Some(stash.clone());
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");

    let db_conv = Conversation::load(local_conversation.local_id.unwrap(), &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conv.num_messages, 10);
}

#[tokio::test]
async fn test_unknown_conversation_messages_returns_error() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    let id = 1024;
    assert_eq!(
        Message::find("WHERE local_conversation_id = ?", params![id], &stash, None)
            .await
            .expect("failed to get messages"),
        vec![]
    );
}

#[tokio::test]
async fn test_conversation_create_starred() {
    let conv_label = ApiConversationLabel {
        id: LabelId::starred().into(),
        context_num_unread: 0,
        context_num_messages: 0,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0,
        context_snooze_time: 0,
    };
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    stash.execute("DELETE FROM labels", vec![]).await.unwrap();
    create_address(&tx).await;
    create_labels(&tx).await;
    test_starred_label().save_using(&tx).await.unwrap();

    // Add starred label, should gain starred attribute.
    let conv = test_conversation(vec![conv_label.clone()], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.stash = Some(stash.clone());
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    {
        let mut local_conversation = Conversation::from(conv.clone());
        local_conversation.stash = Some(stash.clone());
        local_conversation.row_id = Some(1);
        local_conversation.local_id = Some(1);
        local_conversation.labels[0].local_id = Some(1);
        local_conversation.labels[0].local_conversation_id = Some(1);
        local_conversation.labels[0].remote_conversation_id = local_conversation.remote_id.clone();
        local_conversation.labels[0].stash = Some(stash.clone());
        local_conversation.labels[0].row_id = Some(1);
        local_conversation.labels[0].local_label_id = Some(12);
        let db_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        assert_eq!(db_conversation, local_conversation);
        assert!(local_conversation.is_starred());
        assert!(db_conversation.is_starred());
    }
    {
        let mut local_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        local_conversation.labels = vec![ConversationLabel {
            local_id: None,
            local_conversation_id: local_conversation.local_id,
            remote_conversation_id: local_conversation.remote_id.clone(),
            local_label_id: Some(12),
            remote_label_id: LabelId::starred().into(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
            row_id: None,
            stash: None,
        }];
        local_conversation
            .save_using(&tx)
            .await
            .expect("failed to update conversation");
        let db_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        assert_eq!(local_conversation, db_conversation);
        assert!(local_conversation.is_starred());
        assert!(db_conversation.is_starred());
    }

    // Remove starred label, should lose starred attribute.
    let mut local_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    local_conversation.labels = vec![];
    local_conversation.stash = Some(stash.clone());
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();
    {
        let db_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        assert_eq!(db_conversation, local_conversation);
        assert!(!local_conversation.is_starred());
        assert!(!db_conversation.is_starred());
    }
}

#[tokio::test]
async fn test_conversation_create_with_labels() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    let _local_label_ids = create_labels(&tx).await;
    let conv = test_conversation(
        vec![
            ApiConversationLabel {
                id: MY_LABEL_ID1.clone(),
                context_num_unread: 1,
                context_num_messages: 2,
                context_time: 3,
                context_size: 4,
                context_num_attachments: 5,
                context_expiration_time: 6,
                context_snooze_time: 21,
            },
            ApiConversationLabel {
                id: MY_LABEL_ID2.clone(),
                context_num_unread: 7,
                context_num_messages: 8,
                context_time: 9,
                context_size: 10,
                context_num_attachments: 11,
                context_expiration_time: 12,
                context_snooze_time: 31,
            },
        ],
        vec![],
    );
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.labels = vec![ConversationLabel {
        local_id: None,
        local_conversation_id: None,
        remote_conversation_id: Some(MY_LABEL_ID1.clone().into()),
        local_label_id: Some(1),
        remote_label_id: LabelId::starred().into(),
        context_num_unread: 0,
        context_num_messages: 0,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0,
        context_snooze_time: 0,
        row_id: None,
        stash: None,
    }];
    local_conversation.stash = Some(stash.clone());
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(local_conversation, db_conversation);
}

#[tokio::test]
async fn test_conversation_create_with_attachment() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    create_labels(&tx).await;
    let conv = test_conversation(
        vec![],
        vec![ApiAttachmentMetadata {
            id: MY_ATTACHMENT_ID.clone(),
            size: 4098,
            name: "My Attachment.pdf".to_owned(),
            mime_type: ApiMimeType::ApplicationPdf,
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.stash = Some(stash.clone());
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    assert_eq!(local_conversation.attachments_metadata.value.len(), 1);

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation.attachments_metadata.value.len(), 1);
    assert_eq!(
        db_conversation.attachments_metadata.value[0],
        local_conversation.attachments_metadata.value[0],
    );
}

#[tokio::test]
async fn test_conversation_create_with_attachment_and_label() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    let conv = test_conversation(
        vec![ApiConversationLabel {
            id: MY_LABEL_ID1.clone(),
            context_num_unread: 1,
            context_num_messages: 2,
            context_time: 3,
            context_size: 4,
            context_num_attachments: 5,
            context_expiration_time: 6,
            context_snooze_time: 7,
        }],
        vec![ApiAttachmentMetadata {
            id: MY_ATTACHMENT_ID.clone(),
            size: 4098,
            name: "My Attachment.pdf".to_owned(),
            mime_type: ApiMimeType::ApplicationPdf,
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.stash = Some(stash.clone());
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    assert_eq!(local_conversation.attachments_metadata.value.len(), 1);

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    assert_eq!(db_conversation.attachments_metadata.value.len(), 1);
    assert_eq!(
        db_conversation.attachments_metadata.value[0],
        local_conversation.attachments_metadata.value[0],
    );
}

#[tokio::test]
async fn test_conversation_update() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    let _local_label_ids = create_labels(&tx).await;
    let conv = test_conversation(
        vec![ApiConversationLabel {
            id: MY_LABEL_ID2.clone(),
            context_num_unread: 7,
            context_num_messages: 8,
            context_time: 9,
            context_size: 10,
            context_num_attachments: 11,
            context_expiration_time: 12,
            context_snooze_time: 21,
        }],
        vec![ApiAttachmentMetadata {
            id: ApiRemoteId::from("ATTACHMENT2"),
            size: 224515,
            name: "Attachment.json".to_owned(),
            mime_type: ApiMimeType::ApplicationJson,
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation1 = Conversation::from(conv.clone());
    local_conversation1.stash = Some(stash.clone());
    local_conversation1
        .save()
        .await
        .expect("failed to create conversation");
    let conv_update = test_conversation(
        vec![ApiConversationLabel {
            id: MY_LABEL_ID1.clone(),
            context_num_unread: 1,
            context_num_messages: 2,
            context_time: 3,
            context_size: 4,
            context_num_attachments: 5,
            context_expiration_time: 6,
            context_snooze_time: 7,
        }],
        vec![ApiAttachmentMetadata {
            id: MY_ATTACHMENT_ID.clone(),
            size: 4098,
            name: "My Attachment.pdf".to_owned(),
            mime_type: ApiMimeType::ApplicationPdf,
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation2 = Conversation::from(conv_update.clone());
    local_conversation2.labels = vec![
        ConversationLabel {
            local_id: None,
            local_conversation_id: local_conversation2.local_id,
            remote_conversation_id: local_conversation2.remote_id.clone(),
            local_label_id: None,
            remote_label_id: LabelId::starred().into(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
            row_id: None,
            stash: None,
        },
        ConversationLabel {
            local_id: None,
            local_conversation_id: local_conversation2.local_id,
            remote_conversation_id: local_conversation2.remote_id.clone(),
            local_label_id: None,
            remote_label_id: LabelId::starred().into(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
            row_id: None,
            stash: None,
        },
    ];
    local_conversation2.stash = Some(stash.clone());
    local_conversation2.local_id = local_conversation1.local_id;
    local_conversation2.row_id = local_conversation1.row_id;
    local_conversation2
        .save()
        .await
        .expect("failed to update conversation");
    let id = local_conversation2.local_id.unwrap();

    assert_eq!(local_conversation2.attachments_metadata.value.len(), 1);
    local_conversation2.labels.remove(1);

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation, local_conversation2);
}

#[tokio::test]
async fn test_conversation_undelete_all_mail() {
    // Same as test_conversation_delete, but undoing the deletions should restore all the state
    // back to the initial values.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;
    let all_mail_label = Label::find_first(
        "WHERE remote_id = ?",
        params![LabelId::all_mail()],
        tx.stash(),
    )
    .await
    .unwrap()
    .unwrap()
    .local_id
    .unwrap();

    let local_conv_id1 = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_conv_id2 = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();
    Conversation::delete_multiple(vec![local_conv_id1, local_conv_id2], all_mail_label, &tx)
        .await
        .expect("failed to mark as deleted");

    Conversation::delete_multiple(vec![local_conv_id1, local_conv_id2], all_mail_label, &tx)
        .await
        .expect("failed to mark conversations as undeleted");

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_delete_all_mail() {
    // Simulate conversation delete from all mail, all messages for the conversation a
    // are deleted.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;
    let all_mail_label = Label::find_first(
        "WHERE remote_id = ?",
        params![LabelId::all_mail()],
        tx.stash(),
    )
    .await
    .unwrap()
    .unwrap()
    .local_id
    .unwrap();

    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();
    Conversation::delete_multiple(vec![local_conv_id], all_mail_label, &tx)
        .await
        .expect("failed to mark as deleted");

    let mut db_conversation = Conversation::load(local_conv_id, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    db_conversation.deleted = true;
    db_conversation
        .save()
        .await
        .expect("failed to mark as deleted");

    let db_conversation = Conversation::find_first(
        "WHERE local_id = ? AND deleted = 0",
        params![local_conv_id],
        tx.stash(),
    )
    .await
    .expect("failed to get conversation");
    assert!(db_conversation.is_none());

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1,);
            assert_eq!(label_counts.total, start_label_counts.total - 1,);
        }
        // Check conversation label2 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread,);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID1.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
        // Check label2
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID2.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
    }

    // Deleting conv2 should reset all counters to 0.
    let local_conv_id = *state_map
        .conversations
        .get(&state.conversations[1].remote_id.clone().unwrap())
        .unwrap();
    Conversation::delete_multiple(vec![local_conv_id], all_mail_label, &tx)
        .await
        .expect("failed to mark conv as deleted");

    for count in Label::find(String::new(), vec![], tx.stash(), None)
        .await
        .unwrap()
    {
        assert_eq!(
            count.total_msg, 0,
            "Label {:?} does not have 0 total count",
            count.local_id
        );
        assert_eq!(
            count.unread_msg, 0,
            "Label {:?} does not have 0 unread count",
            count.local_id
        );
        assert_eq!(
            count.total_conv, 0,
            "Label {:?} does not have 0 total count",
            count.local_id
        );
        assert_eq!(
            count.unread_conv, 0,
            "Label {:?} does not have 0 unread count",
            count.local_id
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_delete() {
    // Simulate conversation according to API expectations, only delete conversations in that label.
    // If conversation has messages in other labels, it must still exist.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;
    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();
    Conversation::delete_multiple(vec![local_conv_id], local_label_id1, &tx)
        .await
        .expect("failed to mark as deleted");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 0);
    // Should only have one message in other label
    assert_eq!(db_conversation.num_messages, 1);
    assert_eq!(db_conversation.size, state.messages[1].size);
    assert_eq!(
        db_conversation.num_attachments,
        state.messages[1].num_attachments as u64
    );

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID1.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Deleting conv1 in label 2  should remove all traces of the  conversation
    Conversation::delete_multiple(vec![local_conv_id], local_label_id2, &tx)
        .await
        .expect("failed to mark conv as deleted");

    {
        let db_conv = Conversation::load(local_conv_id, &tx)
            .await
            .expect("failed to get conversation");
        assert!(db_conv.is_none());
    }

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, should be empty
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }
        // Check conversation label2 values, should be missing one conversation.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }
        // Check label2 - should be missing one message.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
    }
}

#[tokio::test]
async fn test_conversation_undelete() {
    // Same as test_conversation_delete, but checks for reverse operations.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();
    Conversation::delete_multiple(vec![local_conv_id], local_label_id1, &tx)
        .await
        .expect("failed to mark as deleted");
    Conversation::delete_multiple(vec![local_conv_id], local_label_id2, &tx)
        .await
        .expect("failed to mark as deleted");

    Conversation::undelete_multiple(vec![local_conv_id], local_label_id1, &tx)
        .await
        .expect("Failed to mark as undeleted");
    Conversation::undelete_multiple(vec![local_conv_id], local_label_id2, &tx)
        .await
        .expect("Failed to mark as undeleted");

    let db_conversation = Conversation::load(local_conv_id, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Conversation should match original values.
    {
        let original = &state.conversations[0];
        assert_eq!(db_conversation.num_unread, original.num_unread);
        assert_eq!(db_conversation.num_messages, original.num_messages);
        assert_eq!(db_conversation.size, original.size);
        assert_eq!(db_conversation.num_attachments, original.num_attachments);
    }

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, should match original state.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1 - should match original state.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_counts() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    stash.execute("DELETE FROM labels", vec![]).await.unwrap();
    create_address(&tx).await;
    let labels = create_labels(&tx).await;
    let counts = vec![
        ConversationCount {
            label_id: MY_LABEL_ID1.clone().into(),
            total: 20,
            unread: 4,
        },
        ConversationCount {
            label_id: MY_LABEL_ID2.clone().into(),
            total: 400,
            unread: 124,
        },
    ];

    Label::create_or_update_conversation_counts(counts.clone(), tx.stash())
        .await
        .expect("failed to creat counters");
    let db_labels = Label::find(String::new(), vec![], tx.stash(), None)
        .await
        .expect("failed to get counters");
    let db_counters = db_labels
        .iter()
        .map(|c| ConversationCount {
            label_id: c.remote_id.clone().unwrap(),
            total: c.total_conv,
            unread: c.unread_conv,
        })
        .collect::<Vec<_>>();
    assert!(db_counters.contains(&counts[0]));
    assert!(db_counters.contains(&counts[1]));

    let label_conv_counter = Label::load(labels[0], tx.stash()).await.unwrap().unwrap();
    assert!(db_labels.contains(&label_conv_counter));

    assert_eq!(db_labels.len(), 2);
    assert_eq!(db_labels[0].remote_id, counts[0].label_id.clone().into());
    assert_eq!(db_labels[0].total_conv, counts[0].total);
    assert_eq!(db_labels[0].unread_conv, counts[0].unread);
}

#[tokio::test]
#[ignore]
async fn test_conversation_mark_read_no_message_metadata() {
    // Mark conversation as read without message metadata.
    let mut state = new_test_unread_db_state();
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    let mut db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    db_conversation.num_unread = 0;
    db_conversation
        .save()
        .await
        .expect("failed to save conversation");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_mark_read() {
    // Mark conversation as read and update all conversation / message counts
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    let mut db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    db_conversation.num_unread = 0;
    db_conversation
        .save()
        .await
        .expect("failed to save conversation");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let (unread, _) = message_counts_for_conversation(
                &state.messages,
                state.conversations[0].remote_id.as_ref().unwrap(),
                &MY_LABEL_ID1.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_mark_unread_no_metadata() {
    // Mark conversation as read and then mark it unread, since we don't have message
    // metadata we should mark the current conversation label only as unread.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    let mut db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    db_conversation.num_unread = 0;
    db_conversation.num_unread = db_conversation.num_messages;
    db_conversation
        .save()
        .await
        .expect("failed to save conversation");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts match original values.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        {
            // Label2 should have no unread messages since the message in conv 1 is not the latest.
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_mark_unread() {
    // Mark conversation as read and then mark it unread, only the LATEST message in the
    // conversation should be marked read.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let state = new_test_unread_db_state();
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    let mut db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    db_conversation.num_unread = 0;
    db_conversation.num_unread = db_conversation.num_messages;
    db_conversation
        .save()
        .await
        .expect("failed to save conversation");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts match original values.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        {
            // Label2 should have no unread messages since the message in conv 1 is not the latest.
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_label_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 3);
    assert_eq!(db_conversation.num_attachments, 1);
    assert_eq!(
        db_conversation.size,
        state.messages.iter().fold(0, |x, m| x + m.size)
    );
    assert_eq!(
        db_conversation.expiration_time,
        state
            .messages
            .iter()
            .fold(0, |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 3);
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_double_label_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation twice and check
    // the changes are not duplicated.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 3);
    assert_eq!(db_conversation.num_attachments, 1);
    assert_eq!(
        db_conversation.size,
        state.messages.iter().fold(0, |x, m| x + m.size)
    );
    assert_eq!(
        db_conversation.expiration_time,
        state
            .messages
            .iter()
            .fold(0, |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 3);
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_label_partially() {
    // Label conversation with a label where one of the messages already has been labeled
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let mut state = state.clone();
    state.messages[1]
        .label_ids
        .push(MY_LABEL_ID1.clone().into());
    state.conversations[0].labels.push(ConversationLabel {
        local_id: None,
        local_conversation_id: None,
        remote_conversation_id: None,
        local_label_id: None,
        remote_label_id: Some(MY_LABEL_ID1.clone().into()),
        context_num_unread: 0,
        context_num_messages: 0,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0,
        context_snooze_time: 0,
        row_id: None,
        stash: None,
    });
    let (state, state_map) = prepare_and_patch_db_state(&tx, state).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 3);
    assert_eq!(db_conversation.num_attachments, 1);
    assert_eq!(
        db_conversation.size,
        state.messages.iter().fold(0, |x, m| x + m.size)
    );
    assert_eq!(
        db_conversation.expiration_time,
        state
            .messages
            .iter()
            .fold(0, |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 3);
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_label_without_message_metadata() {
    // Label a conversation with a label that was never assigned without having any message metadata
    // present.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 0);
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(db_conversation.size, 0);
    assert_eq!(db_conversation.expiration_time, 0);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            // unread is 0 due to lack of messages.
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 1);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_double_label_without_message_metadata() {
    // Label a conversation with a label that was never assigned without having any message metadata
    // present 2 times and check the data is not duplicated.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 0);
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(db_conversation.size, 0);
    assert_eq!(db_conversation.expiration_time, 0);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            // unread is 0 due to lack of messages.
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 1);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_conversation_label_without_metadata_uses_information_from_other_labels() {
    // Check that when we label a conversation without message metadata, we
    // grab the maximum value of the other labels this conversation belongs to.
    // There is a fallback to 0 values if no such thing exists. In production
    // conversation will always be assigned to the "All Mail".
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state_label_with_existing_labels();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    let conv_label = &state.conversations[0].labels[0];
    assert_eq!(db_conversation.num_unread, conv_label.context_num_unread);
    assert_eq!(
        db_conversation.num_messages,
        conv_label.context_num_messages
    );
    assert_eq!(
        db_conversation.num_attachments,
        conv_label.context_num_attachments
    );
    assert_eq!(db_conversation.size, conv_label.context_size);
    assert_eq!(
        db_conversation.expiration_time,
        conv_label.context_expiration_time
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            // unread is 0 due to lack of messages.
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 1);
        }
    }
}

#[tokio::test]
async fn test_conversation_unlabel_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::remove_label_from_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to unlabel");

    // Check conversation counts should be 0
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }

    // Check message counts should be 0
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }
}

#[tokio::test]
async fn test_conversation_unlabel_without_message_metadata() {
    // Label and then unlabel a conversation with a label that was never assigned without having any message metadata
    // present.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label_to_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::remove_label_from_multiple(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    // Check conversation counts should be 0
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }
}
