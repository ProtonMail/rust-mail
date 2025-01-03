use std::collections::HashMap;

use crate as proton_mail_common;
use crate::datatypes::{ContextualConversation, ReadFilter};
use crate::models::Conversation;
use crate::models::{CachedConverstationScrollData, ConversationScrollData, Label};
use maplit::hashmap;
use proton_core_common::datatypes::{IdCounterpart, LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_mail_test_utils::db::new_test_connection;
use proton_mail_test_utils::{conv_label, conversation, label, rid};
use stash::orm::Model;
use stash::stash::{Bond, Tether};

fn test_conversations(n: usize, order_shift: u64) -> Vec<Conversation> {
    (0..n)
        .map(|i| {
            let order = i as u64 + order_shift;
            conversation!(remote_id: rid!(i), display_order: order)
        })
        .collect()
}

async fn save_single_conversation(label: &Label, conversation: &mut Conversation, bond: &Bond<'_>) {
    conversation.save(bond).await.unwrap();
    let mut conv_label = conv_label!(
        local_conversation_id: conversation.local_id,
        remote_label_id: label.remote_id.clone(),
        local_label_id: label.local_id,
        context_time: conversation.display_order
    );

    conv_label.save(bond).await.unwrap();
    conversation.reload(bond).await.unwrap();
}

async fn save_to_database(data: &mut HashMap<&str, Vec<Conversation>>, tether: &mut Tether) {
    let bond = tether.transaction().await.unwrap();
    for (label_rid, conversations) in data.iter_mut() {
        let mut label = label!(remote_id: rid!(label_rid));
        label.save(&bond).await.unwrap();
        for conversation in conversations.iter_mut() {
            save_single_conversation(&label, conversation, &bond).await;
        }
    }
    bond.commit().await.unwrap()
}

fn expected_conversations(
    n: usize,
    label_id: &str,
    data: &HashMap<&str, Vec<Conversation>>,
) -> Option<Vec<ContextualConversation>> {
    let convs = data.get(label_id)?;
    // Conversations are read in DESC order
    Some(
        convs
            .iter()
            .rev()
            .take(n)
            .filter_map(|conv| {
                let rid = rid!(label_id);
                let label = conv
                    .labels
                    .iter()
                    .find(|label| label.remote_label_id == rid)?;
                let local_label_id = label.local_label_id?;

                ContextualConversation::new(conv.clone(), local_label_id)
            })
            .collect(),
    )
}

#[tokio::test]
async fn test_scroller_reads_correct_items_within_visible_range() {
    const REMOTE_LABEL_ID: &str = "rid1";

    let stash = new_test_connection().await;
    let mut tether = stash.connection();
    let mut data: HashMap<&str, Vec<Conversation>> = hashmap! {
        REMOTE_LABEL_ID => test_conversations(100, 100),
        "rid2" => test_conversations(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;

    let remote_label_id = RemoteId::from(REMOTE_LABEL_ID);
    let local_label_id = remote_label_id
        .counterpart::<Label>(&tether)
        .await
        .unwrap()
        .unwrap();
    let local_label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;
    let last_conversation = Conversation::find_by_id(RemoteId::from("50"), &tether)
        .await
        .unwrap()
        .unwrap();
    let last_label = last_conversation.label(local_label_id).unwrap();

    let mut scroller = ConversationScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_conversation_id(last_conversation.remote_id.clone().unwrap())
        .conversation_time(last_label.context_time)
        .display_order(last_conversation.display_order)
        .build();

    let bond = tether.transaction().await.unwrap();
    scroller.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    // Test if the scroller can read visible elements
    let expected_count = 50_usize;
    let count = scroller.visible_element_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64);

    let expected = expected_conversations(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected);

    // Test if new scroller read from database returns exactly the same data.
    let new_scroller = ConversationScrollData::find_with_key(local_label_id, unread, &tether)
        .await
        .unwrap()
        .unwrap();

    let count = new_scroller.visible_element_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64);

    let expected = scroller.visible_elements(&tether).await.unwrap();
    let actual = new_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual, expected);

    // Store new conversation outside of the visible view
    let bond = tether.transaction().await.unwrap();
    let mut conversation = conversation!(remote_id: rid!(0), display_order: 0);

    save_single_conversation(&local_label, &mut conversation, &bond).await;

    bond.commit().await.unwrap();

    let expected = expected_conversations(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected_count);
    assert_eq!(actual, expected);

    // Store new conversation inside of the visible view
    // & make sure both scrollers "see" the change
    let bond = tether.transaction().await.unwrap();
    let mut conversation = conversation!(remote_id: rid!(100), display_order: 200);

    save_single_conversation(&local_label, &mut conversation, &bond).await;

    bond.commit().await.unwrap();

    let count = new_scroller.visible_element_count(&tether).await.unwrap();
    assert_eq!(count, expected_count as u64 + 1);

    let expected = scroller.visible_elements(&tether).await.unwrap();
    let actual = new_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual, expected);

    let actual_conv = actual.first().unwrap();
    let expected_conv = ContextualConversation::new(conversation.clone(), local_label_id).unwrap();

    assert_eq!(actual_conv, &expected_conv);

    // Remove just added coversation from inside of the visible view
    let bond = tether.transaction().await.unwrap();
    conversation.delete(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let expected = expected_conversations(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected_count);
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_cashed_scroller_reads_correct_items_within_visible_range() {
    const REMOTE_LABEL_ID: &str = "rid1";

    let stash = new_test_connection().await;
    let mut tether = stash.connection();
    let mut data: HashMap<&str, Vec<Conversation>> = hashmap! {
        REMOTE_LABEL_ID => test_conversations(100, 100),
        "rid2" => test_conversations(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;

    let remote_label_id = RemoteId::from(REMOTE_LABEL_ID);
    let local_label_id = remote_label_id
        .counterpart::<Label>(&tether)
        .await
        .unwrap()
        .unwrap();
    let local_label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;
    let last_conversation = Conversation::find_by_id(RemoteId::from("50"), &tether)
        .await
        .unwrap()
        .unwrap();
    let last_label = last_conversation.label(local_label_id).unwrap();

    let mut scroller = ConversationScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_conversation_id(last_conversation.remote_id.clone().unwrap())
        .conversation_time(last_label.context_time)
        .display_order(last_conversation.display_order)
        .build();

    let bond = tether.transaction().await.unwrap();
    scroller.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let all_count = 50;
    let page_size = 5;
    let mut cached_scroller =
        CachedConverstationScrollData::new(local_label_id, unread, page_size, &tether)
            .await
            .unwrap()
            .unwrap();

    // Test if the scroller can read visible elements within its own range
    let expected_count = 5_usize;
    let count = cached_scroller
        .visible_element_count(&tether)
        .await
        .unwrap();
    assert_eq!(count, expected_count as u64);
    assert!(cached_scroller.has_more(&tether).await.unwrap());

    let expected = expected_conversations(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected);

    // Store new conversation outside of the visible view
    let bond = tether.transaction().await.unwrap();
    let mut conversation = conversation!(remote_id: rid!(0), display_order: 0);

    save_single_conversation(&local_label, &mut conversation, &bond).await;

    bond.commit().await.unwrap();

    let expected = expected_conversations(expected_count, REMOTE_LABEL_ID, &data).unwrap();
    let actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), expected_count);
    assert_eq!(actual, expected);

    // Store new conversation inside of the visible view
    // & make sure cached scroller "see" the change
    let bond = tether.transaction().await.unwrap();
    let mut conversation = conversation!(remote_id: rid!(100), display_order: 200);

    save_single_conversation(&local_label, &mut conversation, &bond).await;

    bond.commit().await.unwrap();

    let count = cached_scroller
        .visible_element_count(&tether)
        .await
        .unwrap();
    assert_eq!(count, expected_count as u64 + 1);
    let expected_conv = ContextualConversation::new(conversation.clone(), local_label_id).unwrap();

    let mut expected = vec![expected_conv.clone()];

    expected.extend(expected_conversations(expected_count, REMOTE_LABEL_ID, &data).unwrap());

    let mut actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual, expected);

    let actual_conv = actual.first().unwrap();

    assert_eq!(actual_conv, &expected_conv);

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
    let bond = tether.transaction().await.unwrap();
    conversation.delete(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let expected = scroller.visible_elements(&tether).await.unwrap();
    let actual = cached_scroller.visible_elements(&tether).await.unwrap();

    assert_eq!(actual.len(), all_count);
    assert_eq!(actual, expected);

    // Create a new cached scroller and assert it starts from the beggining
    let mut cached_scroller =
        CachedConverstationScrollData::new(local_label_id, unread, page_size, &tether)
            .await
            .unwrap()
            .unwrap();
    let expected_count = 5_usize;
    let count = cached_scroller
        .visible_element_count(&tether)
        .await
        .unwrap();

    assert_eq!(count, expected_count as u64);
    assert!(cached_scroller.has_more(&tether).await.unwrap());

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual.first().unwrap().local_id, LocalId::from(100));

    // Delete whole first page
    let convs = data.get(REMOTE_LABEL_ID).unwrap();
    let bond = tether.transaction().await.unwrap();

    for conv_to_delete in convs.iter().rev().take(page_size).cloned() {
        conv_to_delete.delete(&bond).await.unwrap();
    }

    bond.commit().await.unwrap();

    let actual_count = cached_scroller
        .visible_element_count(&tether)
        .await
        .unwrap();
    assert_eq!(actual_count, 0);

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, vec![]);

    // Prove we can progress to the next page
    let actual = cached_scroller.fetch_more(&tether).await.unwrap();
    assert_eq!(actual.len(), page_size);

    let expected = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, expected);

    // Delete next 3 pages and prove we can progress to the next page
    let bond = tether.transaction().await.unwrap();

    for conv_to_delete in convs
        .iter()
        .rev()
        .skip(page_size)
        .take(page_size * 3)
        .cloned()
    {
        conv_to_delete.delete(&bond).await.unwrap();
    }

    bond.commit().await.unwrap();

    let actual_count = cached_scroller
        .visible_element_count(&tether)
        .await
        .unwrap();
    assert_eq!(actual_count, 0);

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, vec![]);

    let actual = cached_scroller.fetch_more(&tether).await.unwrap();
    assert_eq!(actual.len(), page_size);

    let expected = cached_scroller.visible_elements(&tether).await.unwrap();
    assert_eq!(actual, expected);

    // Undelete previous 4 pages
    let convs = data.get_mut(REMOTE_LABEL_ID).unwrap();
    let bond = tether.transaction().await.unwrap();

    for conv in convs.iter_mut().rev().take(page_size * 4) {
        conv.local_id = None;
        conv.row_id = None;
        let mut labels = vec![];
        std::mem::swap(&mut conv.labels, &mut labels);

        conv.save(&bond).await.unwrap();

        for label in labels.iter_mut() {
            label.local_id = None;
            label.row_id = None;
            label.local_conversation_id = conv.local_id;
            label.save(&bond).await.unwrap();
        }

        conv.reload(&bond).await.unwrap();
    }

    bond.commit().await.unwrap();

    let actual = cached_scroller.visible_elements(&tether).await.unwrap();
    let expected = expected_conversations(page_size * 5, REMOTE_LABEL_ID, &data).unwrap();

    assert_eq!(actual.len(), page_size * 5);
    assert_eq!(actual, expected);
}
