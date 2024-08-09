use crate::datatypes::{ConversationCount, MessageAddress, MessageAddresses, MessageCount};
use crate::models::{Conversation, ConversationLabel, Label, Message};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_core_common::models::{Address, ModelExtension};
use stash::orm::Model;
use stash::stash::{Interface, Tether};
use std::collections::{BTreeMap, HashMap};

#[derive(Default, Clone)]
pub struct TestDBState {
    pub addresses: Vec<Address>,
    pub labels: Vec<Label>,
    pub conversations: Vec<Conversation>,
    pub messages: Vec<Message>,
}

#[derive(Default)]
pub struct TestDBStateMap {
    pub labels: HashMap<LabelId, u64>,
    pub conversations: HashMap<RemoteId, u64>,
    pub messages: HashMap<RemoteId, u64>,
    pub conversation_counts: HashMap<LabelId, ConversationCount>,
    pub message_counts: HashMap<LabelId, MessageCount>,
}

pub async fn prepare_db_state_core(tx: &Tether, env: &mut [Address]) {
    // create addresses
    for address in env.iter_mut() {
        address
            .save_using(tx)
            .await
            .expect("failed to create address");
    }
}

pub async fn prepare_and_patch_db_state(
    tx: &Tether,
    env: TestDBState,
) -> (TestDBState, TestDBStateMap) {
    prepare_and_patch_db_state_and_skip(tx, env, false).await
}

pub async fn prepare_and_patch_db_state_and_skip(
    tx: &Tether,
    mut env: TestDBState,
    skip_messages: bool,
) -> (TestDBState, TestDBStateMap) {
    let mut result = TestDBStateMap {
        ..Default::default()
    };
    let stash = tx.stash().clone();

    // create labels
    let mut local_label_ids = vec![];
    for label in env.labels.iter_mut() {
        let db_label = Label::find_by_remote_id(label.remote_id.clone().unwrap().into(), &stash)
            .await
            .expect("failed to find label");
        let the_label = match db_label {
            Some(ref l) => l,
            None => {
                label.set_stash(&stash);
                label.save().await.expect("failed to create label");
                label
            }
        };
        local_label_ids.push(the_label.local_id);
    }
    for (idx, local_id) in local_label_ids.into_iter().enumerate() {
        result.labels.insert(
            env.labels[idx].clone().remote_id.unwrap(),
            local_id.unwrap(),
        );
        result.conversation_counts.insert(
            env.labels[idx].clone().remote_id.unwrap(),
            ConversationCount {
                label_id: env.labels[idx].clone().remote_id.unwrap(),
                total: 0,
                unread: 0,
            },
        );
        result.message_counts.insert(
            env.labels[idx].clone().remote_id.unwrap(),
            MessageCount {
                label_id: env.labels[idx].clone().remote_id.unwrap(),
                total: 0,
                unread: 0,
            },
        );
    }

    // update conversation labels with message data
    fn find_conversation(list: &[Conversation], id: &RemoteId) -> Conversation {
        list.iter()
            .find(|c| c.remote_id == Some(id.clone()))
            .expect("Failed to find conversation")
            .clone()
    }

    fn find_conversation_label(conv: &Conversation, id: &LabelId) -> ConversationLabel {
        conv.labels
            .iter()
            .find(|cl| cl.remote_label_id == Some(id.clone()))
            .expect("Failed to find conversation label")
            .clone()
    }

    fn extend_addresses(addresses: &mut MessageAddresses, new_addresses: &[MessageAddress]) {
        for addr in new_addresses {
            if !addresses
                .value
                .iter_mut()
                .any(|a| a.address == *addr.address)
            {
                addresses.value.push(addr.clone());
            }
        }
    }

    env.messages.sort_by(|m1, m2| {
        if m1.remote_conversation_id == m2.remote_conversation_id {
            return m1.time.cmp(&m2.time);
        }
        m1.remote_conversation_id
            .clone()
            .unwrap()
            .cmp(&m2.remote_conversation_id.clone().unwrap())
    });

    for message in env.messages.iter() {
        let mut conv = find_conversation(
            &env.conversations,
            &message.remote_conversation_id.clone().unwrap(),
        );
        conv.num_attachments += message.num_attachments as u64;
        conv.size += message.size;
        conv.num_messages += 1;
        if message.unread {
            conv.num_unread += 1;
        }
        extend_addresses(&mut conv.senders, &[message.sender.clone()]);
        extend_addresses(&mut conv.recipients, &message.to_list.value);
        extend_addresses(&mut conv.recipients, &message.cc_list.value);

        for label_id in &message.label_ids {
            let mut conv_label = find_conversation_label(&conv, label_id);
            conv_label.context_num_messages += 1;
            conv_label.context_size += message.size;
            conv_label.context_num_attachments += message.num_attachments as u64;
            conv_label.context_time = conv_label.context_time.max(message.time);
            conv_label.context_expiration_time = conv_label
                .context_expiration_time
                .max(message.expiration_time);
            conv_label.context_snooze_time =
                conv_label.context_snooze_time.max(message.snooze_time);
            if message.unread {
                conv_label.context_num_unread += 1;
            }
        }

        if conv.subject.is_empty() {
            conv.subject = message.subject.clone();
        }

        conv.expiration_time = conv.expiration_time.max(message.expiration_time);
    }

    // create conversations
    let local_conversation_ids =
        Conversation::create_or_update_conversations(env.conversations.clone(), &stash)
            .await
            .expect("failed to create conversations");
    for (idx, conversation) in env.conversations.iter().enumerate() {
        result.conversations.insert(
            conversation.remote_id.clone().unwrap(),
            local_conversation_ids[idx],
        );

        for label in &conversation.labels {
            let counts = result
                .conversation_counts
                .get_mut(&label.remote_label_id.clone().unwrap())
                .unwrap();
            if label.context_num_unread != 0 {
                counts.unread += 1
            }
            counts.total += 1;
        }
    }

    // create messages
    if !skip_messages {
        let mut local_message_ids = vec![];
        for message in env.messages.iter_mut() {
            message.set_stash(&stash);
            message.save().await.expect("failed to create message");
            local_message_ids.push(message.local_id);
            result.messages.insert(
                message.remote_id.clone().unwrap(),
                message.local_id.unwrap(),
            );

            for label_id in &message.label_ids {
                let counts = result.message_counts.get_mut(label_id).unwrap();
                if message.unread {
                    counts.unread += 1
                }
                counts.total += 1
            }
        }
    }

    // create conversation_counts
    Label::create_or_update_conversation_counts(
        result.conversation_counts.values().cloned().collect(),
        &stash,
    )
    .await
    .expect("failed to create conversation counts");
    if !skip_messages {
        Label::create_or_update_message_counts(
            result.message_counts.values().cloned().collect(),
            &stash,
        )
        .await
        .expect("failed to create message counts");
    }

    (env, result)
}

pub fn find_conversation_label(conv: &Conversation, id: &LabelId) -> ConversationLabel {
    conv.labels
        .iter()
        .find(|cl| cl.remote_label_id == Some(id.clone()))
        .expect("Failed to find conversation label")
        .clone()
}

pub fn message_counts_for_conversation(
    messages: &[Message],
    conversation_id: &RemoteId,
    label_id: &LabelId,
) -> (u64, u64) {
    let mut unread = 0_u64;
    let mut total = 0_u64;

    for m in messages {
        if m.remote_conversation_id.clone().unwrap() != *conversation_id {
            continue;
        }

        if !m.label_ids.contains(label_id) {
            continue;
        }

        total += 1;
        if m.unread {
            unread += 1;
        }
    }

    (unread, total)
}

pub async fn conv_counts_as_map(tx: &Tether) -> BTreeMap<u64, ConversationCount> {
    BTreeMap::from_iter(
        Label::all(tx.stash(), None)
            .await
            .unwrap()
            .into_iter()
            .map(|c| {
                (
                    c.local_id.unwrap(),
                    ConversationCount {
                        label_id: c.remote_id.clone().unwrap(),
                        total: c.total_conv,
                        unread: c.unread_conv,
                    },
                )
            }),
    )
}

pub async fn msg_counts_as_map(tx: &Tether) -> BTreeMap<u64, MessageCount> {
    BTreeMap::from_iter(
        Label::all(tx.stash(), None)
            .await
            .unwrap()
            .into_iter()
            .map(|c| {
                (
                    c.local_id.unwrap(),
                    MessageCount {
                        label_id: c.remote_id.clone().unwrap(),
                        total: c.total_msg,
                        unread: c.unread_msg,
                    },
                )
            }),
    )
}
