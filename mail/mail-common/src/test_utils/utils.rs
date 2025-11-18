use crate::datatypes::{
    ConversationLabelsCount, LocalConversationId, LocalMessageId, MessageLabelsCount,
    MessageRecipient, MessageRecipients, MessageSender, MessageSenders,
};
use crate::models::{
    Conversation, ConversationCounters, ConversationLabel, Message, MessageCounters,
};
use crate::test_utils::search::MY_ADDRESS_ID;
use futures::{FutureExt as _, StreamExt};
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressFlags, AddressStatus as ApiAddressStatus,
    AddressType as ApiAddressType, LabelId,
};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Address, Label, ModelExtension, ModelIdExtension};
use proton_crypto_account::keys::AddressKeys as ApiAddressKeys;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use stash::orm::Model;
use stash::stash::{StashError, Tether};
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
    pub labels: HashMap<LabelId, LocalLabelId>,
    pub conversations: HashMap<ConversationId, LocalConversationId>,
    pub messages: HashMap<MessageId, LocalMessageId>,
    pub conversation_counts: HashMap<LabelId, ConversationLabelsCount>,
    pub message_counts: HashMap<LabelId, MessageLabelsCount>,
}

pub async fn prepare_db_state_core(tether: &mut Tether, env: &mut [Address]) {
    tether
        .tx::<_, _, StashError>(async |tx| {
            for address in env.iter_mut() {
                address.save(tx).await.unwrap();
            }
            Ok(())
        })
        .await
        .expect("failed to commit transaction");
}

pub async fn prepare_and_patch_db_state(
    tether: &mut Tether,
    env: TestDBState,
) -> (TestDBState, TestDBStateMap) {
    prepare_and_patch_db_state_and_skip(tether, env, false).await
}

#[allow(clippy::too_many_lines)]
pub async fn prepare_and_patch_db_state_and_skip(
    tether: &mut Tether,
    mut env: TestDBState,
    skip_messages: bool,
) -> (TestDBState, TestDBStateMap) {
    let mut result = TestDBStateMap {
        ..Default::default()
    };
    tether
        .tx::<_, _, StashError>(async |tx| {
            // create labels
            let mut local_label_ids = vec![];
            for label in &mut env.labels {
                let db_label = Label::find_by_remote_id(
                    label.remote_id.clone().expect("No remote id in label"),
                    tx,
                )
                .await
                .expect("failed to find label");
                let the_label = if let Some(ref l) = db_label {
                    l
                } else {
                    label.save(tx).await.expect("failed to create label");
                    label
                };
                local_label_ids.push(the_label.id());
            }
            for (idx, local_id) in local_label_ids.into_iter().enumerate() {
                result
                    .labels
                    .insert(env.labels[idx].clone().remote_id.unwrap(), local_id);
                result.conversation_counts.insert(
                    env.labels[idx].clone().remote_id.unwrap(),
                    ConversationLabelsCount {
                        label_id: env.labels[idx].clone().remote_id.unwrap(),
                        total: 0,
                        unread: 0,
                    },
                );
                result.message_counts.insert(
                    env.labels[idx].clone().remote_id.unwrap(),
                    MessageLabelsCount {
                        label_id: env.labels[idx].clone().remote_id.unwrap(),
                        total: 0,
                        unread: 0,
                    },
                );
            }
            Ok(())
        })
        .await
        .expect("failed to commit transaction");
    // update conversation labels with message data
    #[allow(clippy::items_after_statements)]
    fn find_conversation<'a>(
        list: &'a mut [Conversation],
        id: &ConversationId,
    ) -> &'a mut Conversation {
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
    fn extend_senders(addresses: &mut MessageSenders, new_addresses: &[MessageSender]) {
        for addr in new_addresses {
            if !addresses
                .value
                .iter_mut()
                .any(|a| a.address == addr.address)
            {
                addresses.value.push(addr.clone());
            }
        }
    }

    #[allow(clippy::items_after_statements)]
    fn extend_recipients(recipients: &mut MessageRecipients, new_recipients: &[MessageRecipient]) {
        for addr in new_recipients {
            if !recipients
                .value
                .iter_mut()
                .any(|a| a.address == addr.address)
            {
                recipients.value.push(addr.clone());
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
            .as_str()
            .cmp(m2.remote_conversation_id.clone().unwrap().as_str())
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
        extend_senders(&mut conv.senders, &[message.sender.clone()]);
        extend_recipients(&mut conv.recipients, &message.to_list.value);
        extend_recipients(&mut conv.recipients, &message.cc_list.value);

        for label_id in &message.label_ids {
            let conv_label = find_conversation_label(conv, label_id);
            conv_label.context_num_messages += 1;
            conv_label.context_size += message.size;
            conv_label.context_num_attachments += u64::from(message.num_attachments);
            conv_label.context_time = conv_label.context_time.max(message.time);
            conv_label
                .context_expiration_time
                .merge(message.expiration_time);
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

    tether
        .tx::<_, _, StashError>(async |tx| {
            // create conversations
            let local_conversation_ids =
                Conversation::create_or_update_conversations(env.conversations.clone(), tx)
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
                    message.save(tx).await.expect("failed to create message");
                    local_message_ids.push(message.local_id);
                    result
                        .messages
                        .insert(message.remote_id.clone().unwrap(), message.id());

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
            ConversationLabelsCount::create_or_update_conversation_counts(
                result.conversation_counts.values().cloned().collect(),
                tx,
            )
            .await
            .expect("failed to create conversation counts");
            if !skip_messages {
                MessageLabelsCount::create_or_update_message_counts(
                    result.message_counts.values().cloned().collect(),
                    tx,
                )
                .await
                .expect("failed to create message counts");
            }

            Ok((env, result))
        })
        .await
        .unwrap()
}

#[must_use]
pub fn find_conversation_label(conv: &Conversation, id: &LabelId) -> ConversationLabel {
    conv.labels
        .iter()
        .find(|cl| cl.remote_label_id == Some(id.clone()))
        .expect("Failed to find conversation label")
        .clone()
}

#[must_use]
pub fn message_counts_for_conversation(
    messages: &[Message],
    conversation_id: &ConversationId,
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

#[allow(clippy::from_iter_instead_of_collect)]
pub async fn conv_counts_as_map(
    tether: &Tether,
) -> BTreeMap<LocalLabelId, ConversationLabelsCount> {
    let iter = ConversationCounters::all(tether)
        .await
        .unwrap()
        .into_iter()
        .map(|counter| {
            let label_id = Label::resolve_remote_label_id(counter.local_label_id, tether);
            label_id.map(|id| (counter, id))
        });

    futures::stream::FuturesUnordered::from_iter(iter)
        .map(|(c, label_id)| {
            (
                c.local_label_id,
                ConversationLabelsCount {
                    label_id: label_id.unwrap(),
                    total: c.total,
                    unread: c.unread,
                },
            )
        })
        .collect()
        .await
}

#[allow(clippy::from_iter_instead_of_collect)]
pub async fn msg_counts_as_map(tether: &Tether) -> BTreeMap<LocalLabelId, MessageLabelsCount> {
    let iter = MessageCounters::all(tether)
        .await
        .unwrap()
        .into_iter()
        .map(|counter| {
            let label_id = Label::resolve_remote_label_id(counter.local_label_id, tether);
            label_id.map(|id| (counter, id))
        });

    futures::stream::FuturesUnordered::from_iter(iter)
        .map(|(c, label_id)| {
            (
                c.local_label_id,
                MessageLabelsCount {
                    label_id: label_id.unwrap(),
                    total: c.total,
                    unread: c.unread,
                },
            )
        })
        .collect()
        .await
}

pub async fn create_address(tether: &mut Tether) -> Address {
    let mut address = test_address();
    tether
        .tx::<_, _, StashError>(async |tx| address.save(tx).await)
        .await
        .unwrap();
    address
}

pub const TEST_ADDRESS_EMAIL: &str = "hello@world";

#[must_use]
pub fn test_address() -> Address {
    Address::from(test_api_address())
}

#[must_use]
pub fn test_api_address() -> ApiAddress {
    ApiAddress {
        id: MY_ADDRESS_ID.clone(),
        email: TEST_ADDRESS_EMAIL.to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 0,
        display_name: "HelloWorld".to_owned(),
        signature: "SIGNATURE".to_owned(),
        keys: ApiAddressKeys::new(vec![]),
        catch_all: false,
        proton_mx: false,
        signed_key_list: Default::default(),
        flags: AddressFlags::default(),
    }
}
