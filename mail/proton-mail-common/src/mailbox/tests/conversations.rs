use crate::avatar::AvatarInformation;
use crate::db::{
    LabelColor, LocalConversationId, LocalLabel, LocalLabelId, LocalMessageId, LocalMessageMetadata,
};
use crate::mailbox::conversation::first_unread_message_in_conversation;
use proton_api_mail::domain::{LabelId, LabelType, MessageFlags};
use proton_api_mail::proton_api_core::domain::AddressId;

#[test]
fn first_unread_conversation_message_in_starred_or_custom_label_or_folder() {
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread
    let messages = [
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(3), MessageFlags::RECEIVED, true),
    ];

    let label_starred = new_label(LabelType::System, Some(LabelId::starred().clone()));
    let label_label = new_label(LabelType::Label, None);
    let label_folder = new_label(LabelType::Folder, None);

    let unread_id = first_unread_message_in_conversation(&label_starred, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(2)));
    let unread_id = first_unread_message_in_conversation(&label_folder, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(2)));
    let unread_id = first_unread_message_in_conversation(&label_label, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(2)));
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
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::empty(), true),
        message_metadata_with_flags(LocalMessageId::new(3), MessageFlags::RECEIVED, true),
    ];

    let label_folder = new_label(LabelType::Folder, None);

    let unread_id = first_unread_message_in_conversation(&label_folder, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(3)));

    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread + Draft
    let messages = [
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(3), MessageFlags::empty(), true),
    ];

    let unread_id = first_unread_message_in_conversation(&label_folder, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(2)));

    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread + Draft
    let messages = [
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::empty(), true),
    ];

    let unread_id = first_unread_message_in_conversation(&label_folder, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(0)));
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
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(3), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(4), MessageFlags::RECEIVED, false),
    ];

    let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    let unread_id = first_unread_message_in_conversation(&label, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(2)));
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
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(3), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(4), MessageFlags::empty(), true),
    ];

    let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    let unread_id = first_unread_message_in_conversation(&label, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(2)));
    // Messages are tagged as
    // 0: unread
    // 1: read
    // 2: unread
    // 3: unread
    // 4: unread + Auto Send
    let messages = [
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(3), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(
            LocalMessageId::new(4),
            MessageFlags::SENT | MessageFlags::AUTO,
            true,
        ),
    ];

    let unread_id = first_unread_message_in_conversation(&label, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(2)));
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
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, false),
        message_metadata_with_flags(
            LocalMessageId::new(2),
            MessageFlags::SENT | MessageFlags::AUTO,
            true,
        ),
        message_metadata_with_flags(LocalMessageId::new(3), MessageFlags::empty(), true),
        message_metadata_with_flags(LocalMessageId::new(4), MessageFlags::RECEIVED, false),
    ];

    let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    let unread_id = first_unread_message_in_conversation(&label, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(0)));
}

#[test]
fn oldest_unread_message_selected_in_unread_chain() {
    // Messages are tagged as
    // 0: unread
    // 1: unread
    // 2: unread
    let messages = [
        message_metadata_with_flags(LocalMessageId::new(0), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(1), MessageFlags::RECEIVED, true),
        message_metadata_with_flags(LocalMessageId::new(2), MessageFlags::RECEIVED, true),
    ];

    let label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
    let unread_id = first_unread_message_in_conversation(&label, &messages);
    assert_eq!(unread_id, Some(LocalMessageId::new(0)));
}

fn message_metadata_with_flags(
    id: LocalMessageId,
    flags: MessageFlags,
    unread: bool,
) -> LocalMessageMetadata {
    LocalMessageMetadata {
        id,
        rid: None,
        conversation_id: LocalConversationId::new(0),
        address_id: AddressId::from(""),
        order: 0,
        subject: String::new(),
        unread,
        sender: Default::default(),
        to: vec![],
        cc: vec![],
        bcc: vec![],
        time: 0,
        size: 0,
        expiration_time: 0,
        snooze_time: 0,
        is_replied: false,
        is_replied_all: false,
        is_forwarded: false,
        external_id: None,
        num_attachments: 0,
        flags,
        starred: false,
        attachments: None,
        labels: None,
        avatar_information: AvatarInformation {
            text: String::new(),
            color: String::new(),
        },
    }
}

fn new_label(label_type: LabelType, rid: Option<LabelId>) -> LocalLabel {
    LocalLabel {
        id: LocalLabelId::new(0),
        rid,
        parent_id: None,
        name: String::new(),
        path: None,
        color: LabelColor::black(),
        label_type,
        order: 0,
        notify: false,
        expanded: false,
        sticky: false,
    }
}
