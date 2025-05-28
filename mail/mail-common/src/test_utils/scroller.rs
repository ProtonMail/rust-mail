use std::{collections::HashMap, fmt::Display};

use proton_core_common::models::{Label, ModelExtension};
use stash::{
    orm::Model,
    stash::{Bond, StashError, Tether},
};

use crate::{
    conv_id, conv_label, conversation, label, lbl_id, message,
    models::{Conversation, ConversationCounters, Message, MessageCounters},
    msg_id,
};

use super::utils::create_address;

pub fn test_messages(n: usize, order_shift: u64) -> Vec<Message> {
    (0..n)
        .map(|i| {
            let order = i as u64 + order_shift;
            message!(remote_id: msg_id!(format!("mymsg_{order}")),  display_order: order, time: order.into())
        })
        .collect()
}

pub async fn save_single_message(label: &[Label], message: &mut Message, bond: &Bond<'_>) {
    message.label_ids = label.iter().map(|l| l.remote_id.clone().unwrap()).collect();
    message.save(bond).await.unwrap();
    message.reload(bond).await.unwrap();
}

pub fn test_conversations(n: usize, order_shift: u64) -> Vec<Conversation> {
    (0..n)
        .map(|i| {
            let order = i as u64 + order_shift;
            conversation!(remote_id: conv_id!(format!("myconv_{order}")), display_order: order)
        })
        .collect()
}

pub async fn save_single_conversation(
    labels: &[Label],
    conversation: &mut Conversation,
    bond: &Bond<'_>,
) {
    conversation.save(bond).await.unwrap();
    for label in labels {
        let mut conv_label = conv_label!(
            local_conversation_id: conversation.local_id,
            remote_label_id: label.remote_id.clone(),
            local_label_id: label.local_id,
            context_time: 0.into()
        );
        conv_label.save(bond).await.unwrap();
    }

    conversation.reload(bond).await.unwrap();
}

pub trait StoreLabeledModelMap<D: Display, T: Model> {
    fn save_to_database(
        &mut self,
        tether: &mut Tether,
    ) -> impl std::future::Future<Output = ()> + Send;
}

impl<D: Display + Send + Sync> StoreLabeledModelMap<D, Conversation>
    for HashMap<Vec<D>, Vec<Conversation>>
{
    async fn save_to_database(&mut self, tether: &mut Tether) {
        tether
            .tx::<_, _, StashError>(async |tx| {
                for (label_ids, conversations) in self.iter_mut() {
                    let mut labels = Vec::new();
                    for label_id in label_ids {
                        let mut label = label!(remote_id: lbl_id!(label_id.to_string()));
                        label.save(tx).await.unwrap();
                        let mut counters = ConversationCounters::new(label.local_id.unwrap());
                        counters.total = conversations.len() as u64;
                        counters.save(tx).await.unwrap();
                        labels.push(label);
                    }
                    for conversation in conversations.iter_mut() {
                        save_single_conversation(&labels, conversation, tx).await;
                    }
                }
                Ok(())
            })
            .await
            .unwrap();
    }
}

impl<D: Display + Send + Sync> StoreLabeledModelMap<D, Message> for HashMap<Vec<D>, Vec<Message>> {
    async fn save_to_database(&mut self, tether: &mut Tether) {
        let address = create_address(tether).await;
        tether
            .tx::<_, _, StashError>(async |bond| {
                let mut conv =
                    conversation!(remote_id: conv_id!("unique_conv_id_for_storing_messages"));
                conv.save(bond).await.unwrap();
                for (label_ids, messages) in self.iter_mut() {
                    let mut labels = Vec::new();
                    for label_id in label_ids {
                        let mut label = label!(remote_id: lbl_id!(label_id.to_string()));
                        label.save(bond).await.unwrap();
                        let mut counters = MessageCounters::new(label.local_id.unwrap());
                        counters.total = messages.len() as u64;
                        counters.save(bond).await.unwrap();
                        labels.push(label);
                    }
                    for message in messages.iter_mut() {
                        message.local_address_id = address.local_id.unwrap();
                        message.remote_address_id = address.remote_id.clone().unwrap();
                        message.local_conversation_id = conv.local_id;
                        message.remote_conversation_id = conv.remote_id.clone();
                        save_single_message(&labels, message, bond).await;
                    }
                }
                Ok(())
            })
            .await
            .unwrap();
    }
}
