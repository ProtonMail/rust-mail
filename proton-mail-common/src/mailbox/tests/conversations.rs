use crate::datatypes::{LabelColor, LabelType, MessageAddress, MessageFlags, SystemLabelId};
use crate::models::{Conversation, Label, Message};
use proton_core_common::datatypes::{LabelId, RemoteId};

#[test]
fn first_unread_conversation_message_in_starred_or_custom_label_or_folder() {
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(3, MessageFlags::RECEIVED, true),
    ];

    let label_starred = new_label(LabelType::System, Some(LabelId::starred().clone()));
    let label_label = new_label(LabelType::Label, None);
    let label_folder = new_label(LabelType::Folder, None);

    let unread_id = Conversation::first_unread_message(&label_starred, &messages);
    assert_eq!(unread_id, Some(2));
    let unread_id = Conversation::first_unread_message(&label_folder, &messages);
    assert_eq!(unread_id, Some(2));
    let unread_id = Conversation::first_unread_message(&label_label, &messages);
    assert_eq!(unread_id, Some(2));
}

#[test]
fn first_unread_conversation_message_in_starred_or_custom_label_or_folder_non_consecutive_with_draft(
) {
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread + Draft
    // 3: unread
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::empty(), true),
        message_metadata_with_flags(3, MessageFlags::RECEIVED, true),
    ];

    let label_folder = new_label(LabelType::Folder, None);

    let unread_id = Conversation::first_unread_message(&label_folder, &messages);
    assert_eq!(unread_id, Some(3));

    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread + Draft
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(3, MessageFlags::empty(), true),
    ];

    let unread_id = Conversation::first_unread_message(&label_folder, &messages);
    assert_eq!(unread_id, Some(2));

    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread + Draft
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::empty(), true),
    ];

    let unread_id = Conversation::first_unread_message(&label_folder, &messages);
    assert_eq!(unread_id, Some(0));
}

#[test]
fn first_unread_conversation_message_default_last_consecutive_unread() {
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread
    // 4: read
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(3, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(4, MessageFlags::RECEIVED, false),
    ];

    let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    let unread_id = Conversation::first_unread_message(&label, &messages);
    assert_eq!(unread_id, Some(2));
}

#[test]
fn first_unread_conversation_message_default_last_consecutive_unread_if_last_is_draft_or_auto_send()
{
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread
    // 4: unread + Draft
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(3, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(4, MessageFlags::empty(), true),
    ];

    let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    let unread_id = Conversation::first_unread_message(&label, &messages);
    assert_eq!(unread_id, Some(2));
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread
    // 4: unread + Auto Send
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(3, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(4, MessageFlags::SENT | MessageFlags::AUTO, true),
    ];

    let unread_id = Conversation::first_unread_message(&label, &messages);
    assert_eq!(unread_id, Some(2));
}

#[test]
fn first_unread_conversation_message_default_last_nonconsecutive_not_draft_or_auto_send() {
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread + Auto Send
    // 3: unread + Draft
    // 4: read
    let messages = [
        message_metadata_with_flags(0, MessageFlags::RECEIVED, true),
        message_metadata_with_flags(1, MessageFlags::RECEIVED, false),
        message_metadata_with_flags(2, MessageFlags::SENT | MessageFlags::AUTO, true),
        message_metadata_with_flags(3, MessageFlags::empty(), true),
        message_metadata_with_flags(4, MessageFlags::RECEIVED, false),
    ];

    let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    let unread_id = Conversation::first_unread_message(&label, &messages);
    assert_eq!(unread_id, Some(0));
}

// #[test]
// fn oldest_unread_message_selected_in_unread_chain() {
//     // Messages are tagged as
//     // 0: unread
//     // 1: unread
//     // 2: unread
//     let messages = [
//         message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
//         message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, true),
//         message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::RECEIVED, true),
//     ];
//
//     let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
//     let unread_id = first_unread_message_in_conversation(&label, &messages);
//     assert_eq!(unread_id, Some(LocalMessageId::new(0)));
// }

fn message_metadata_with_flags(id: u64, flags: MessageFlags, unread: bool) -> Message {
    Message {
        local_id: Some(id),
        remote_id: None,
        local_conversation_id: None,
        remote_conversation_id: None,
        address_id: RemoteId::from(""),
        display_order: 0,
        parsed_headers: Default::default(),
        subject: String::new(),
        unread,
        row_id: None,
        sender: MessageAddress {
            address: String::new(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: String::new(),
        },
        time: 0,
        size: 0,
        expiration_time: 0,
        snooze_time: 0,
        is_replied: false,
        is_replied_all: false,
        label_ids: Default::default(),
        is_forwarded: false,
        external_id: None,
        num_attachments: 0,
        flags,
        attachments: Default::default(),
        attachments_metadata: Default::default(),
        bcc_list: Default::default(),
        body: String::new(),
        cc_list: Default::default(),
        header: String::new(),
        mime_type: Default::default(),
        reply_tos: Default::default(),
        to_list: Default::default(),
        stash: None,
        deleted: false,
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
