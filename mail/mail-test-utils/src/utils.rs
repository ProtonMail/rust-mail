use crate::search::MY_ADDRESS_ID;
use proton_core_common::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType,
};
use proton_core_common::datatypes::{LabelId, LocalId, RemoteId};
use proton_core_common::models::{Address, ModelExtension};
use proton_mail_common::datatypes::{
    ConversationCount, MessageAddress, MessageAddresses, MessageCount,
};
use proton_mail_common::models::{Conversation, ConversationLabel, Label, Message};
use rand::{distributions::Uniform, Rng};
use stash::orm::Model;
use stash::stash::{Interface, Tether};
use std::collections::{BTreeMap, HashMap};

#[derive(Default, Clone, Debug)]
pub struct TestDBState {
    pub addresses: Vec<Address>,
    pub labels: Vec<Label>,
    pub conversations: Vec<Conversation>,
    pub messages: Vec<Message>,
}

#[derive(Default, Debug)]
pub struct TestDBStateMap {
    pub labels: HashMap<LabelId, LocalId>,
    pub conversations: HashMap<RemoteId, LocalId>,
    pub messages: HashMap<RemoteId, LocalId>,
    pub conversation_counts: HashMap<LabelId, ConversationCount>,
    pub message_counts: HashMap<LabelId, MessageCount>,
}

/// # Panics
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

/// # Panics
#[allow(clippy::too_many_lines)]
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
    for label in &mut env.labels {
        let db_label = Label::find_by_id(
            RemoteId::from(label.remote_id.clone().expect("No remote id in label")),
            &stash,
        )
        .await
        .expect("failed to find label");
        let the_label = if let Some(ref l) = db_label {
            l
        } else {
            label.set_stash(&stash);
            label.save().await.expect("failed to create label");
            label
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
    #[allow(clippy::items_after_statements)]
    fn find_conversation<'a>(list: &'a mut [Conversation], id: &RemoteId) -> &'a mut Conversation {
        list.iter_mut()
            .find(|c| c.remote_id == Some(id.clone()))
            .expect("Failed to find conversation")
    }

    #[allow(clippy::items_after_statements)]
    fn find_conversation_label<'a>(
        conv: &'a mut Conversation,
        id: &LabelId,
    ) -> &'a mut ConversationLabel {
        conv.labels
            .iter_mut()
            .find(|cl| cl.remote_label_id == Some(id.clone()))
            .expect("Failed to find conversation label")
    }

    #[allow(clippy::items_after_statements)]
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

    for message in &env.messages {
        let conv = find_conversation(
            &mut env.conversations,
            &message.remote_conversation_id.clone().unwrap(),
        );
        conv.num_attachments += u64::from(message.num_attachments);
        conv.size += message.size;
        conv.num_messages += 1;
        if message.unread {
            conv.num_unread += 1;
        }
        extend_addresses(&mut conv.senders, &[message.sender.clone()]);
        extend_addresses(&mut conv.recipients, &message.to_list.value);
        extend_addresses(&mut conv.recipients, &message.cc_list.value);

        for label_id in &message.label_ids {
            let conv_label = find_conversation_label(conv, label_id);
            conv_label.context_num_messages += 1;
            conv_label.context_size += message.size;
            conv_label.context_num_attachments += u64::from(message.num_attachments);
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
            conv.subject.clone_from(&message.subject);
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
                counts.unread += 1;
            }
            counts.total += 1;
        }
    }

    // create messages
    if !skip_messages {
        let mut local_message_ids = vec![];
        for message in &mut env.messages {
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
                    counts.unread += 1;
                }
                counts.total += 1;
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

/// # Panics
#[must_use]
pub fn find_conversation_label(conv: &Conversation, id: &LabelId) -> ConversationLabel {
    conv.labels
        .iter()
        .find(|cl| cl.remote_label_id == Some(id.clone()))
        .expect("Failed to find conversation label")
        .clone()
}

/// # Panics
#[must_use]
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

/// # Panics
#[allow(clippy::from_iter_instead_of_collect)]
pub async fn conv_counts_as_map(tx: &Tether) -> BTreeMap<LocalId, ConversationCount> {
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

/// # Panics
#[allow(clippy::from_iter_instead_of_collect)]
pub async fn msg_counts_as_map(tx: &Tether) -> BTreeMap<LocalId, MessageCount> {
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

/// # Panics
pub async fn create_address(core_tx: &Tether) -> Address {
    let mut address = test_address();
    address
        .save_using(core_tx)
        .await
        .expect("failed to create address");

    address
}

#[must_use]
pub fn test_address() -> Address {
    Address {
        local_id: None,
        remote_id: Some(MY_ADDRESS_ID.clone().into()),
        email: "hello@world".to_owned(),
        send: Default::default(),
        receive: Default::default(),
        status: AddressStatus::Enabled,
        domain_id: None,
        address_type: AddressType::Original,
        display_order: 0,
        display_name: "HelloWorld".to_owned(),
        signature: "SIGNATURE".to_owned(),
        keys: AddressKeys::default(),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList {
            min_epoch_id: None,
            max_epoch_id: None,
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: None,
            revision: 0,
        },
        row_id: None,
        stash: None,
    }
}

/// Generates a random string of the specified length, including alphanumeric and special characters.
///
/// # Parameters
/// - `length`: The length of the string to generate.
#[must_use]
pub fn random_string(length: usize) -> String {
    let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                           abcdefghijklmnopqrstuvwxyz\
                           0123456789!@#$%^&*()_+-=[]{}|;:'\",.<>?/\\`~";

    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.sample(Uniform::new(0, charset.len()));
            charset[idx] as char
        })
        .collect()
}
