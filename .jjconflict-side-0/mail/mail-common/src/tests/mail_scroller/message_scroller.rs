use std::collections::BTreeMap;

use crate as proton_mail_common;
use crate::datatypes::LocalMessageId;
use crate::datatypes::ReadFilter;
use crate::datatypes::labels::ScrollOrderDir;
use crate::datatypes::labels::ScrollOrderField;
use crate::models::{CachedScrollData, MessageScrollData, ScrollCursor};
use crate::models::{Message, ScrollData};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_common::test_utils::db::new_test_connection;
use proton_mail_common::test_utils::utils::create_address;
use proton_mail_common::{conv_id, conversation, label, lbl_id, message, msg_id};
use stash::orm::Model;
use stash::stash::{Bond, StashError, Tether};
use velcro::btree_map;

fn test_message(n: usize, order_shift: u64) -> Vec<Message> {
    (0..n)
        .map(|i| {
            let order = i as u64 + order_shift;
            message!(remote_id: msg_id!(order),  display_order: order, time: order.into())
        })
        .collect()
}

async fn save_single_message(label: &Label, message: &mut Message, bond: &Bond<'_>) {
    message.label_ids = vec![label.remote_id.clone().unwrap()];
    message.save(bond).await.unwrap();
    message.reload(bond).await.unwrap();
}

async fn save_to_database(data: &mut BTreeMap<&str, Vec<Message>>, tether: &mut Tether) {
    let address = create_address(tether).await;
    tether
        .tx::<_, _, StashError>(async |bond| {
            let mut conv = conversation!(remote_id: conv_id!("convid_1"));
            conv.save(bond).await.unwrap();
            for (label_rid, messages) in data.iter_mut() {
                let mut label = label!(remote_id: lbl_id!(label_rid));
                label.save(bond).await.unwrap();
                for message in messages.iter_mut() {
                    message.local_address_id = address.id();
                    message.remote_address_id = address.remote_id.clone().unwrap();
                    message.local_conversation_id = conv.local_id;
                    message.remote_conversation_id = conv.remote_id.clone();
                    save_single_message(&label, message, bond).await;
                }
            }
            Ok(())
        })
        .await
        .unwrap();
}

fn expected_messages(
    n: usize,
    label_id: &str,
    data: &BTreeMap<&str, Vec<Message>>,
) -> Option<Vec<Message>> {
    let convs = data.get(label_id)?;
    Some(convs.iter().rev().take(n).cloned().collect())
}

#[tokio::test]
async fn test_scroller_reads_correct_items_within_visible_range() {
    const REMOTE_LABEL_ID: &str = "rid1";

    let stash = new_test_connection().await;
    let mut tether = stash.connection().await.unwrap();
    let mut data = btree_map! {
        REMOTE_LABEL_ID: test_message(100, 100),
        "rid2": test_message(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;

    let remote_label_id = LabelId::from(REMOTE_LABEL_ID);
    let local_label_id = Label::resolve_local_label_id(remote_label_id, &tether)
        .await
        .unwrap();
    let local_label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;
    let last_message = Message::find_by_remote_id(MessageId::from("150"), &tether)
        .await
        .unwrap()
        .unwrap();

    let mut scroller = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_message_id(last_message.remote_id.clone().unwrap())
        .message_time(last_message.time)
        .snooze_time(last_message.snooze_time)
        .display_order(last_message.display_order)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx::<_, _, StashError>(async |bond| {
            scroller.save(bond).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    let scroller: ScrollCursor<MessageScrollData> = scroller.into();

    // Test if the scroller can read visible elements
    let expected_count = 50_usize;
    let count = scroller.seen_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64);

    let expected = expected_messages(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected);

    // Test if new scroller read from database returns exactly the same data.
    let new_scroller: ScrollCursor<MessageScrollData> =
        MessageScrollData::find_with_key(local_label_id, unread, ScrollOrderDir::Desc, &tether)
            .await
            .unwrap()
            .unwrap()
            .into();

    let count = new_scroller.seen_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64);

    let expected = scroller.visible_elements(&tether).await.unwrap();
    let actual = new_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual, expected);

    // Store new conversation outside of the visible view
    tether
        .tx::<_, _, StashError>(async |bond| {
            let mut message = data.get(REMOTE_LABEL_ID).unwrap().first().cloned().unwrap();
            message.local_id = None;
            message.remote_id = msg_id!(51);
            message.display_order = 0;
            message.time = 0.into();
            save_single_message(&local_label, &mut message, bond).await;
            Ok(())
        })
        .await
        .unwrap();

    let expected = expected_messages(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected_count);
    assert_eq!(actual, expected);

    // Store new conversation inside of the visible view
    // & make sure both scrollers "see" the change
    let mut message = data.get(REMOTE_LABEL_ID).unwrap().first().cloned().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            message.local_id = None;
            message.remote_id = msg_id!(300);
            message.display_order = 300;
            message.time = 300.into();
            save_single_message(&local_label, &mut message, bond).await;
            Ok(())
        })
        .await
        .unwrap();

    let count = new_scroller.seen_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64 + 1);

    let expected = scroller.visible_elements(&tether).await.unwrap();
    let actual = new_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual, expected);

    let actual_msg = actual.first().unwrap();

    assert_eq!(actual_msg, &message);

    // Remove just added coversation from inside of the visible view
    tether
        .tx(async |bond| message.delete(bond).await)
        .await
        .unwrap();

    let expected = expected_messages(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected_count);
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_cashed_scroller_reads_correct_items_within_visible_range() {
    const REMOTE_LABEL_ID: &str = "rid1";

    let stash = new_test_connection().await;
    let mut tether = stash.connection().await.unwrap();
    let mut data = btree_map! {
        REMOTE_LABEL_ID: test_message(100, 100),
        "rid2": test_message(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;

    let remote_label_id = LabelId::from(REMOTE_LABEL_ID);
    let local_label_id = Label::resolve_local_label_id(remote_label_id, &tether)
        .await
        .unwrap();
    let local_label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;
    let last_message = Message::find_by_remote_id(MessageId::from("150"), &tether)
        .await
        .unwrap()
        .unwrap();

    let mut scroller = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_message_id(last_message.remote_id.clone().unwrap())
        .message_time(last_message.time)
        .snooze_time(last_message.snooze_time)
        .display_order(last_message.display_order)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx(async |bond| scroller.save(bond).await)
        .await
        .unwrap();
    let scroller = ScrollCursor::from(scroller);

    let all_count = 50;
    let page_size = 5;
    let mut cached_scroller =
        CachedScrollData::<MessageScrollData>::new(local_label_id, unread, page_size, &tether)
            .await
            .unwrap()
            .unwrap();
    cached_scroller.fetch_more(&tether).await.unwrap();
    // Test if the scroller can read visible elements within its own range
    let expected_count = 5_usize;
    let count = cached_scroller.seen_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64);
    assert!(cached_scroller.has_more(&tether).await.unwrap());

    let expected = expected_messages(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected);

    // Store new conversation outside of the visible view
    let mut message = data.get(REMOTE_LABEL_ID).unwrap().first().cloned().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            message.local_id = None;
            message.remote_id = msg_id!(51);
            message.display_order = 0;
            message.time = 0.into();
            save_single_message(&local_label, &mut message, bond).await;
            Ok(())
        })
        .await
        .unwrap();

    let expected = expected_messages(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected_count);
    assert_eq!(actual, expected);

    // Store new conversation inside of the visible view
    // & make sure cached scroller "see" the change
    let mut message = data.get(REMOTE_LABEL_ID).unwrap().first().cloned().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            message.local_id = None;
            message.remote_id = msg_id!(300);
            message.display_order = 300;
            message.time = 300.into();
            save_single_message(&local_label, &mut message, bond).await;
            Ok(())
        })
        .await
        .unwrap();

    let count = cached_scroller.seen_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64 + 1);

    let mut expected = vec![message.clone()];

    expected.extend(expected_messages(expected_count, REMOTE_LABEL_ID, &data).unwrap());

    let mut actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual, expected);

    let actual_msg = actual.first().unwrap();

    assert_eq!(actual_msg, &message);

    // Progress the cached scroller
    // Use previously loaded items & extend them with the new loaded page
    actual.extend(cached_scroller.fetch_more(&tether).await.unwrap());
    let expected = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual, expected);

    // Progress cached scroller to the end
    while cached_scroller.has_more(&tether).await.unwrap() {
        cached_scroller.fetch_more(&tether).await.unwrap();
    }

    let expected = scroller.visible_elements(&tether).await.unwrap();
    let actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), all_count + 1);
    assert_eq!(actual, expected);

    // Remove just added coversation from inside of the visible view
    tether
        .tx::<_, _, StashError>(async |bond| message.delete(bond).await)
        .await
        .unwrap();

    let expected = scroller.visible_elements(&tether).await.unwrap();
    let actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), all_count);
    assert_eq!(actual, expected);

    // Create a new cached scroller and assert it starts from the beggining
    let mut cached_scroller =
        CachedScrollData::<MessageScrollData>::new(local_label_id, unread, page_size, &tether)
            .await
            .unwrap()
            .unwrap();
    cached_scroller.fetch_more(&tether).await.unwrap();
    let expected_count = 5_usize;
    let count = cached_scroller.seen_count(&tether).await.unwrap();

    assert_eq!(count, expected_count as u64);
    assert!(cached_scroller.has_more(&tether).await.unwrap());

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(
        actual.first().unwrap().local_id,
        Some(LocalMessageId::from(100))
    );

    // Delete whole first page
    let messages = data.get(REMOTE_LABEL_ID).unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            for msg_to_delete in messages.iter().rev().take(page_size).cloned() {
                msg_to_delete.delete(bond).await.unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();

    let actual_count = cached_scroller.seen_count(&tether).await.unwrap();
    assert_eq!(actual_count, 0);

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, vec![]);

    // Prove we can progress to the next page
    let actual = cached_scroller.fetch_more(&tether).await.unwrap();
    assert_eq!(actual.len(), page_size);

    let expected = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, expected);

    // Delete next 3 pages and prove we can progress to the next page
    tether
        .tx::<_, _, StashError>(async |bond| {
            for msg_to_delete in messages
                .iter()
                .rev()
                .skip(page_size)
                .take(page_size * 3)
                .cloned()
            {
                msg_to_delete.delete(bond).await.unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();

    let actual_count = cached_scroller.seen_count(&tether).await.unwrap();
    assert_eq!(actual_count, 0);

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, vec![]);

    let actual = cached_scroller.fetch_more(&tether).await.unwrap();
    assert_eq!(actual.len(), page_size);

    let expected = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, expected);

    // Undelete previous 4 pages
    let msgs = data.get_mut(REMOTE_LABEL_ID).unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            for msg in msgs.iter_mut().rev().take(page_size * 4) {
                msg.local_id = None;

                msg.save(bond).await.unwrap();
                msg.reload(bond).await.unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    let expected = expected_messages(page_size * 5, REMOTE_LABEL_ID, &data).unwrap();

    assert_eq!(actual.len(), page_size * 5);
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_cashed_scroller_reads_last_two_pages_together_when_last_page_is_not_filled() {
    const REMOTE_LABEL_ID: &str = "rid1";

    let stash = new_test_connection().await;
    let mut tether = stash.connection().await.unwrap();
    let mut data = btree_map! {
        REMOTE_LABEL_ID: test_message(5, 100),
        "rid2": test_message(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;

    let remote_label_id = LabelId::from(REMOTE_LABEL_ID);
    let local_label_id = Label::resolve_local_label_id(remote_label_id, &tether)
        .await
        .unwrap();
    let unread = ReadFilter::All;
    let last_message = data.get(REMOTE_LABEL_ID).unwrap().first().unwrap();

    let mut scroller = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_message_id(last_message.remote_id.clone().unwrap())
        .message_time(last_message.time)
        .snooze_time(last_message.snooze_time)
        .display_order(last_message.display_order)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx::<_, _, StashError>(async |bond| scroller.save(bond).await)
        .await
        .unwrap();

    let page_size = 2;
    let mut cached_scroller =
        CachedScrollData::<MessageScrollData>::new(local_label_id, unread, page_size, &tether)
            .await
            .unwrap()
            .unwrap();

    let items = cached_scroller.seen_count(&tether).await.unwrap();

    assert_eq!(items, 0);

    cached_scroller.fetch_more(&tether).await.unwrap();

    let items = cached_scroller.seen_count(&tether).await.unwrap();

    assert_eq!(items, 2);

    let loaded_page = cached_scroller.fetch_more(&tether).await.unwrap();
    assert_eq!(loaded_page.len(), 3);

    let items = cached_scroller.seen_count(&tether).await.unwrap();

    assert_eq!(items, 5);
}

#[tokio::test]
async fn allow_different_filter_types_to_be_stored_in_database() {
    let stash = new_test_connection().await;
    let mut tether = stash.connection().await.unwrap();
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut scroller_all = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::All)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .display_order(0)
        .build();

    let mut scroller_read = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::Read)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    let mut scroller_unread = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::Unread)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();
    let count = MessageScrollData::all_count(&tether).await.unwrap();

    assert_eq!(count, 0);

    tether
        .tx::<_, _, StashError>(async |bond| {
            scroller_all.save(bond).await.unwrap();
            scroller_read.save(bond).await.unwrap();
            scroller_unread.save(bond).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let count = MessageScrollData::all_count(&tether).await.unwrap();

    assert_eq!(count, 3);
    assert_eq!(scroller_all.id, Some(1));
    assert_eq!(scroller_read.id, Some(2));
    assert_eq!(scroller_unread.id, Some(3));

    let all = MessageScrollData::all(&tether).await.unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.contains(&scroller_all));
    assert!(all.contains(&scroller_read));
    assert!(all.contains(&scroller_unread));

    tether
        .tx::<_, _, StashError>(async |bond| {
            scroller_all.save(bond).await.unwrap();
            scroller_read.save(bond).await.unwrap();
            scroller_unread.save(bond).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let count = MessageScrollData::all_count(&tether).await.unwrap();

    assert_eq!(count, 3);
    assert_eq!(scroller_all.id, Some(1));
    assert_eq!(scroller_read.id, Some(2));
    assert_eq!(scroller_unread.id, Some(3));

    let mut scroller_all = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::All)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    let mut scroller_read = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::Read)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    let mut scroller_unread = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::Unread)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx::<_, _, StashError>(async |bond| {
            scroller_all.save(bond).await.unwrap();
            scroller_read.save(bond).await.unwrap();
            scroller_unread.save(bond).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let count = MessageScrollData::all_count(&tether).await.unwrap();

    assert_eq!(count, 3);
    assert_eq!(scroller_all.id, Some(1));
    assert_eq!(scroller_read.id, Some(2));
    assert_eq!(scroller_unread.id, Some(3));

    let mut scroller_all = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::All)
        .remote_message_id(MessageId::from("150"))
        .message_time(1.into())
        .snooze_time(1.into())
        .display_order(2)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    let mut scroller_read = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::Read)
        .remote_message_id(MessageId::from("150"))
        .message_time(1.into())
        .snooze_time(1.into())
        .display_order(2)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    let mut scroller_unread = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(ReadFilter::Unread)
        .remote_message_id(MessageId::from("150"))
        .message_time(1.into())
        .snooze_time(1.into())
        .display_order(2)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx::<_, _, StashError>(async |bond| {
            scroller_all.save(bond).await.unwrap();
            scroller_read.save(bond).await.unwrap();
            scroller_unread.save(bond).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let count = MessageScrollData::all_count(&tether).await.unwrap();

    assert_eq!(count, 3);
    assert_eq!(scroller_all.id, Some(1));
    assert_eq!(scroller_read.id, Some(2));
    assert_eq!(scroller_unread.id, Some(3));

    scroller_all.reload(&tether).await.unwrap();
    scroller_read.reload(&tether).await.unwrap();
    scroller_unread.reload(&tether).await.unwrap();

    assert_eq!(scroller_all.message_time, 1.into());
    assert_eq!(scroller_all.display_order, 2);
    assert_eq!(scroller_read.message_time, 1.into());
    assert_eq!(scroller_read.display_order, 2);
    assert_eq!(scroller_unread.message_time, 1.into());
    assert_eq!(scroller_unread.display_order, 2);

    let diff_local_label_id = SystemLabel::AllMail
        .local_id(&tether)
        .await
        .unwrap()
        .unwrap();
    let mut scroller_all = MessageScrollData::builder()
        .local_label_id(diff_local_label_id)
        .unread(ReadFilter::All)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    let mut scroller_read = MessageScrollData::builder()
        .local_label_id(diff_local_label_id)
        .unread(ReadFilter::Read)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    let mut scroller_unread = MessageScrollData::builder()
        .local_label_id(diff_local_label_id)
        .unread(ReadFilter::Unread)
        .remote_message_id(MessageId::from("150"))
        .message_time(0.into())
        .snooze_time(0.into())
        .display_order(0)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx::<_, _, StashError>(async |bond| {
            scroller_all.save(bond).await.unwrap();
            scroller_read.save(bond).await.unwrap();
            scroller_unread.save(bond).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let count = MessageScrollData::all_count(&tether).await.unwrap();

    assert_eq!(count, 6);
    assert_eq!(scroller_all.id, Some(4));
    assert_eq!(scroller_read.id, Some(5));
    assert_eq!(scroller_unread.id, Some(6));
}
