use crate::db::{u64, u64, u64, ConversationCount, MailSqliteConnectionMut, MessageCount};
use proton_api_mail::{
    domain::{
        Conversation, ConversationCount, ConversationId, ConversationLabels, Label, LabelId,
        MessageAddress, MessageCount, MessageId, MessageMetadata,
    },
    proton_api_core::domain::Address,
};
use proton_core_common::db::CoreSqliteConnectionMut;
use std::collections::{BTreeMap, HashMap};

#[derive(Default, Clone)]
pub(in crate::db::conversations) struct TestDBState {
    pub addresses: Vec<Address>,
    pub labels: Vec<Label>,
    pub conversations: Vec<Conversation>,
    pub messages: Vec<MessageMetadata>,
}

#[derive(Default)]
pub(in crate::db::conversations) struct TestDBStateMap {
    pub labels: HashMap<LabelId, u64>,
    pub conversations: HashMap<ConversationId, u64>,
    pub messages: HashMap<MessageId, u64>,
    pub conversation_counts: HashMap<LabelId, ConversationCount>,
    pub message_counts: HashMap<LabelId, MessageCount>,
}

pub(in crate::db::conversations) fn prepare_db_state_core(
    tx: &mut CoreSqliteConnectionMut,
    env: &[Address],
) {
    // create addresses
    tx.create_or_update_addresses(env.iter())
        .expect("failed to create addresses");
}

pub(in crate::db::conversations) fn prepare_and_patch_db_state(
    tx: &mut MailSqliteConnectionMut,
    env: TestDBState,
) -> (TestDBState, TestDBStateMap) {
    prepare_and_patch_db_state_and_skip(tx, env, false)
}

pub(in crate::db::conversations) fn prepare_and_patch_db_state_and_skip(
    tx: &mut MailSqliteConnectionMut,
    mut env: TestDBState,
    skip_messages: bool,
) -> (TestDBState, TestDBStateMap) {
    let mut result = TestDBStateMap {
        ..Default::default()
    };

    // create labels
    let local_label_ids = tx
        .create_remote_labels(env.labels.iter())
        .expect("failed to create labels");
    for (idx, label) in local_label_ids.into_iter().enumerate() {
        result.labels.insert(env.labels[idx].id.clone(), label);
        result.conversation_counts.insert(
            env.labels[idx].id.clone(),
            ConversationCount {
                label_id: env.labels[idx].id.clone(),
                total: 0,
                unread: 0,
            },
        );
        result.message_counts.insert(
            env.labels[idx].id.clone(),
            MessageCount {
                label_id: env.labels[idx].id.clone(),
                total: 0,
                unread: 0,
            },
        );
    }

    // update conversation labels with message data
    fn find_conversation<'a>(
        list: &'a mut [Conversation],
        id: &ConversationId,
    ) -> &'a mut Conversation {
        list.iter_mut()
            .find(|c| c.id == *id)
            .expect("Failed to find conversation")
    }

    fn find_conversation_label<'a>(
        conv: &'a mut Conversation,
        id: &LabelId,
    ) -> &'a mut ConversationLabels {
        conv.labels
            .iter_mut()
            .find(|cl| cl.id == *id)
            .expect("Failed to find conversation label")
    }

    fn extend_addresses<'i>(
        addresses: &mut Vec<MessageAddress>,
        new_addresses: impl Iterator<Item = &'i MessageAddress>,
    ) {
        for addr in new_addresses {
            if addresses
                .iter_mut()
                .find(|a| a.address == *addr.address)
                .is_none()
            {
                addresses.push(addr.clone());
            }
        }
    }

    env.messages.sort_by(|m1, m2| {
        if m1.conversation_id == m2.conversation_id {
            return m1.time.cmp(&m2.time);
        }
        m1.conversation_id
            .as_ref()
            .cmp(&m2.conversation_id.as_ref())
    });

    for message in env.messages.iter() {
        let conv = find_conversation(&mut env.conversations, &message.conversation_id);
        conv.num_attachments += message.num_attachments as u64;
        conv.size += message.size;
        conv.num_messages += 1;
        if message.unread == true {
            conv.num_unread += 1;
        }
        extend_addresses(&mut conv.senders, std::iter::once(&message.sender));
        extend_addresses(&mut conv.recipients, message.to_list.iter());
        extend_addresses(&mut conv.recipients, message.cc_list.iter());

        for label_id in &message.label_ids {
            let conv_label = find_conversation_label(conv, label_id);
            conv_label.context_num_messages += 1;
            conv_label.context_size += message.size;
            conv_label.context_num_attachments += message.num_attachments as u64;
            conv_label.context_time = conv_label.context_time.max(message.time);
            conv_label.context_expiration_time = conv_label
                .context_expiration_time
                .max(message.expiration_time);
            conv_label.context_snooze_time =
                conv_label.context_snooze_time.max(message.snooze_time);
            if message.unread == true {
                conv_label.context_num_unread += 1;
            }
        }

        if conv.subject.is_empty() {
            conv.subject = message.subject.clone();
        }

        conv.expiration_time = conv.expiration_time.max(message.expiration_time);
    }

    // create conversations
    let local_conversation_ids = tx
        .create_conversations(env.conversations.iter())
        .expect("failed to create conversations");
    for (idx, conversation) in env.conversations.iter().enumerate() {
        result
            .conversations
            .insert(conversation.id.clone(), local_conversation_ids[idx]);

        for label in &conversation.labels {
            let counts = result.conversation_counts.get_mut(&label.id).unwrap();
            if label.context_num_unread != 0 {
                counts.unread += 1
            }
            counts.total += 1;
        }
    }

    // create messages
    if !skip_messages {
        let local_message_ids = tx
            .create_messages_from_metadata(env.messages.iter())
            .expect("failed to create conversations");
        for (idx, message) in env.messages.iter().enumerate() {
            result
                .messages
                .insert(message.id.clone(), local_message_ids[idx]);

            for label_id in &message.label_ids {
                let counts = result.message_counts.get_mut(label_id).unwrap();
                if message.unread == true {
                    counts.unread += 1
                }
                counts.total += 1
            }
        }
    }

    // create conversation_counts
    tx.create_or_update_conversation_counts(result.conversation_counts.values())
        .expect("failed to create conversation counts");
    if !skip_messages {
        tx.create_or_update_message_counts(result.message_counts.values())
            .expect("failed to create message counts");
    }

    (env, result)
}

pub(in crate::db::conversations) fn find_conversation_label<'a>(
    conv: &'a Conversation,
    id: &LabelId,
) -> &'a ConversationLabels {
    conv.labels
        .iter()
        .find(|cl| cl.id == *id)
        .expect("Failed to find conversation label")
}

pub(in crate::db::conversations) fn message_counts_for_conversation(
    messages: &[MessageMetadata],
    conversation_id: &ConversationId,
    label_id: &LabelId,
) -> (u64, u64) {
    let mut unread = 0_u64;
    let mut total = 0_u64;

    for m in messages {
        if m.conversation_id != *conversation_id {
            continue;
        }

        if !m.label_ids.contains(label_id) {
            continue;
        }

        total += 1;
        if m.unread == true {
            unread += 1;
        }
    }

    return (unread, total);
}

pub(in crate::db::conversations) fn conv_counts_as_map(
    tx: &mut MailSqliteConnectionMut,
) -> BTreeMap<u64, ConversationCount> {
    BTreeMap::from_iter(
        tx.conversation_counts()
            .unwrap()
            .into_iter()
            .map(|c| (c.id.clone(), c)),
    )
}

pub(in crate::db::conversations) fn msg_counts_as_map(
    tx: &mut MailSqliteConnectionMut,
) -> BTreeMap<u64, MessageCount> {
    BTreeMap::from_iter(
        tx.message_counts()
            .unwrap()
            .into_iter()
            .map(|c| (c.id.clone(), c)),
    )
}
