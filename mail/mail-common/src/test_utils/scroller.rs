use std::{collections::HashMap, fmt::Display};

use proton_core_api::services::proton::LabelId;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use stash::{
    orm::Model,
    params,
    stash::{Bond, StashError, Tether},
};

use crate::{
    conv_id, conversation, label, lbl_id, message,
    models::{Conversation, ConversationCounters, Message, MessageCounters},
    msg_id,
};

use super::utils::{create_address, test_address};

pub const UNIQUE_CONV_ID: &str = "unique_conv_id_for_storing_messages";

/// Create a vector of `n` messages with display order and time shifted by `order_shift`.
/// Also the remote id is derived from the order. This requires user of the function to
/// ensure that the (n..(n+order_shift)) is unique across all messages.
///
pub fn test_messages(n: usize, order_shift: usize) -> Vec<Message> {
    (0..n)
        .map(|order| {
            let order = (order + order_shift) as u64;
            message!(remote_id: msg_id!(format!("mymsg_{order}")),  display_order: order, time: order.into())
        })
        .collect()
}

pub async fn save_single_message(labels: &[Label], message: &mut Message, bond: &Bond<'_>) {
    message.save(bond).await.unwrap();
    let local_message_id = message.local_id.unwrap();
    for label_id in labels.iter().map(|l| l.local_id.unwrap()) {
        Message::apply_label(label_id, vec![local_message_id], bond)
            .await
            .unwrap();
    }
    message.reload(bond).await.unwrap();
}

pub async fn create_single_message(
    conv: &Conversation,
    postfix: impl Display,
    tether: &mut Tether,
) -> Message {
    let conv_id = conv
        .remote_id
        .clone()
        .map(|id| id.to_string())
        .unwrap_or_default();
    let address = create_address(tether).await;
    let mut message = message!(remote_id: msg_id!(format!("{conv_id}_msg_{postfix}")));
    message.local_address_id = address.local_id.unwrap();
    message.remote_address_id = address.remote_id.clone().unwrap();
    message.local_conversation_id = message.local_conversation_id.or(conv.local_id);
    message.remote_conversation_id = message
        .remote_conversation_id
        .clone()
        .or(conv.remote_id.clone());
    let labels = conv.load_labels(tether).await.unwrap();

    tether
        .tx(async |tx| {
            save_single_message(&labels, &mut message, tx).await;
            Result::<Message, StashError>::Ok(message)
        })
        .await
        .unwrap()
}

/// Create a vector of `n` conversations with display order shifted by `order_shift`.
/// Also the remote id is derived from the order. This requires user of the function to
/// ensure that the (n..(n+order_shift)) is unique across all conversations.
///
pub fn test_conversations(n: usize, order_shift: usize) -> Vec<Conversation> {
    (0..n)
        .map(|order| {
            let order = (order + order_shift) as u64;
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
    let local_conversation_id = conversation.local_id.unwrap();
    for label_id in labels.iter().map(|l| l.local_id.unwrap()) {
        Conversation::apply_label(label_id, vec![local_conversation_id], bond)
            .await
            .unwrap();
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
            .tx::<_, _, StashError>(async |bond| {
                for (label_ids, conversations) in self.iter_mut() {
                    let mut labels = Vec::new();
                    for label_id in label_ids {
                        let remote_label_id = LabelId::from(label_id.to_string());
                        let mut label = {
                            match Label::find_by_remote_id(remote_label_id.clone(), bond).await? {
                                Some(x) => x,
                                None => create_label(bond, label_id.to_string()).await?,
                            }
                        };
                        label.save(bond).await.unwrap();

                        let mut counters = ConversationCounters::find_first(
                            "WHERE local_label_id = ?",
                            params![label.local_id.unwrap()],
                            bond,
                        )
                        .await
                        .unwrap()
                        .unwrap_or_else(|| ConversationCounters::new(label.local_id.unwrap()));

                        for conversation in conversations.iter_mut() {
                            counters.total += 1;
                            counters.unread += if conversation.num_unread > 0 { 1 } else { 0 };
                        }
                        counters.save(bond).await.unwrap();

                        labels.push(label);
                    }

                    for conversation in conversations.iter_mut() {
                        save_single_conversation(&labels, conversation, bond).await;
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
                let mut conv = conversation!(remote_id: conv_id!(UNIQUE_CONV_ID));
                conv.save(bond).await.unwrap();
                for (label_ids, messages) in self.iter_mut() {
                    let mut labels = Vec::new();
                    for label_id in label_ids {
                        labels.push(create_label(bond, label_id.to_string()).await?);
                    }
                    for message in messages.iter_mut() {
                        message.local_address_id = address.local_id.unwrap();
                        message.remote_address_id = address.remote_id.clone().unwrap();
                        message.local_conversation_id =
                            message.local_conversation_id.or(conv.local_id);
                        message.remote_conversation_id = message
                            .remote_conversation_id
                            .clone()
                            .or(conv.remote_id.clone());
                        save_single_message(&labels, message, bond).await;
                    }
                }
                Ok(())
            })
            .await
            .unwrap();
    }
}

async fn create_label(bond: &Bond<'_>, label_id: impl Into<LabelId>) -> Result<Label, StashError> {
    let label_id = label_id.into();
    let label = match Label::find_by_remote_id(label_id.clone(), bond).await? {
        Some(label) => label,
        None => {
            let mut label = label!(remote_id: lbl_id!(label_id));
            label.save(bond).await?;
            label
        }
    };

    let mut counters = ConversationCounters::new(label.local_id.unwrap());
    counters.save(bond).await?;

    let mut counters = MessageCounters::new(label.local_id.unwrap());
    counters.save(bond).await?;

    Ok(label)
}

impl<D> StoreLabeledModelMap<D, Message> for (D, Vec<Message>)
where
    D: Display + Send,
{
    async fn save_to_database(&mut self, tether: &mut Tether) {
        let label = self.0.to_string();
        tether
            .tx::<_, _, StashError>(async |bond| {
                let mut conv =
                    conversation!(remote_id: conv_id!("unique_conv_id_for_storing_messages"));
                conv.save(bond).await.unwrap();
                let mut address = test_address();
                address.save(bond).await.unwrap();

                let remote_label_id = LabelId::from(label.clone());

                for message in &mut self.1 {
                    if let Some(conv_id) = &message.remote_conversation_id {
                        let mut conv = Conversation::find_by_remote_id(conv_id.clone(), bond)
                            .await
                            .unwrap()
                            .unwrap();
                        message.local_conversation_id = conv.local_id;
                        conv.num_messages += 1;
                        conv.num_unread += if message.unread { 1 } else { 0 };
                        conv.save(bond).await.unwrap();
                    } else {
                        message.local_conversation_id = conv.local_id;
                        conv.num_messages += 1;
                        conv.num_unread += if message.unread { 1 } else { 0 };
                        conv.save(bond).await.unwrap();
                    }

                    // If the label already exists, we don't need to create it
                    let mut label = Label::find_by_remote_id(remote_label_id.clone(), bond)
                        .await?
                        .unwrap_or_else(|| label!(remote_id: Some(remote_label_id.clone())));
                    label.save(bond).await.unwrap();

                    {
                        let mut counters = MessageCounters::find_first(
                            "WHERE local_label_id = ?",
                            params![label.local_id.unwrap()],
                            bond,
                        )
                        .await
                        .unwrap()
                        .unwrap_or_else(|| MessageCounters::new(label.local_id.unwrap()));
                        counters.total += 1;
                        counters.unread += if message.unread { 1 } else { 0 };
                        counters.save(bond).await.unwrap();
                    }

                    message.local_address_id = address.local_id.unwrap();
                    message.remote_address_id = address.remote_id.clone().unwrap();

                    save_single_message(&[label], message, bond).await;
                }
                Ok(())
            })
            .await
            .unwrap();
    }
}
