use crate::datatypes::{LabelColor, LabelType, MessageAddress, MessageFlags, SystemLabelId};
use crate::models::{Conversation, Label, Message};
use proton_core_common::datatypes::LabelId;

use lazy_static::lazy_static;
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
    expected_id: Option<LocalId>,
) {
    let messages = messages
        .iter()
        .enumerate()
        .map(|(id, (flags, unread))| message_metadata_with_flags((id as u64).into(), *flags, *unread))
        .collect::<Vec<_>>();

    for label in labels {
        assert_eq!(
            Conversation::first_unread_message(label, &messages),
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
