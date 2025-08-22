use std::{collections::HashMap, fmt::Display, time::Duration};

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

use crate::{
    MailContextError, MailUserContext,
    mail_scroller::{MailScroller, MailScrollerHandle, ScrollerUpdate},
};

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
    let local_message_id = message.id();
    for label_id in labels.iter().map(|l| l.id()) {
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
    message.local_address_id = address.id();
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
    let local_conversation_id = conversation.id();
    for label_id in labels.iter().map(|l| l.id()) {
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
                            params![label.id()],
                            bond,
                        )
                        .await
                        .unwrap()
                        .unwrap_or_else(|| ConversationCounters::new(label.id()));

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
                        message.local_address_id = address.id();
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

    let mut counters = ConversationCounters::new(label.id());
    counters.save(bond).await?;

    let mut counters = MessageCounters::new(label.id());
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
                            params![label.id()],
                            bond,
                        )
                        .await
                        .unwrap()
                        .unwrap_or_else(|| MessageCounters::new(label.id()));
                        counters.total += 1;
                        counters.unread += if message.unread { 1 } else { 0 };
                        counters.save(bond).await.unwrap();
                    }

                    message.local_address_id = address.id();
                    message.remote_address_id = address.remote_id.clone().unwrap();

                    save_single_message(&[label], message, bond).await;
                }
                Ok(())
            })
            .await
            .unwrap();
    }
}

const TIMEOUT: Duration = Duration::from_secs(5);
/// Generic helper struct to manage scroller and its updates in tests
///
/// This provides a unified interface for testing any type of MailScroller
/// (conversations, messages, search) with automatic update handling and
/// convenient test methods.
pub struct TestScroller<T: Send + Sync + Clone + Eq + std::fmt::Debug + 'static> {
    scroller: MailScroller,
    handle: MailScrollerHandle<T>,
    collected_items: Vec<T>,
}

impl<T: Send + Sync + Clone + Eq + std::fmt::Debug + 'static> TestScroller<T> {
    /// Create a new TestScroller from a MailScroller and handle
    pub async fn new(
        scroller: MailScroller,
        handle: MailScrollerHandle<T>,
    ) -> Result<Self, MailContextError> {
        let mut test_scroller = Self {
            scroller,
            handle,
            collected_items: Vec::new(),
        };

        // Wait for any initial updates that might be available immediately
        // This handles cases where cached data is loaded during scroller initialization
        test_scroller
            .try_wait_for_update(Duration::from_secs(1))
            .await?;

        Ok(test_scroller)
    }

    /// Send a fetch_more command to the scroller
    pub fn fetch_more(&self) -> Result<(), MailContextError> {
        self.scroller.fetch_more()
    }

    /// Send a refresh command to the scroller
    pub fn refresh(&self) -> Result<(), MailContextError> {
        self.scroller.refresh()
    }

    /// Send a force_refresh command to the scroller
    pub fn force_refresh(&self) -> Result<(), MailContextError> {
        self.scroller.force_refresh()
    }

    /// Check if the scroller has more items available
    pub async fn has_more(&self) -> Result<bool, MailContextError> {
        self.scroller.has_more().await
    }

    /// Get the total number of items available
    pub async fn total(&self) -> Result<u64, MailContextError> {
        self.scroller.total().await
    }

    /// Get the number of items already seen
    pub async fn seen(&self) -> Result<u64, MailContextError> {
        self.scroller.seen().await
    }

    /// Wait for and process the next update, returning the items that were added/changed
    pub async fn wait_for_update(&mut self) -> Result<Option<Vec<T>>, MailContextError> {
        let update = self.handle.updates.recv_async().await.map_err(|_| {
            MailContextError::Other(anyhow::anyhow!("Failed to receive scroller update"))
        })?;

        self.handle_scroller_update(update)
    }

    /// Try to wait for an update with a timeout, returning None if timeout is reached
    pub async fn try_wait_for_update(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<T>>, MailContextError> {
        match tokio::time::timeout(timeout, self.wait_for_update()).await {
            Ok(update) => update,
            _ => Ok(None),
        }
    }

    /// Fetch more and wait for the update (or handle timeout gracefully)
    pub async fn fetch_more_and_wait(&mut self) -> Result<Vec<T>, MailContextError> {
        self.fetch_more()?;

        // Try to get an update, but if none comes within reasonable time, return empty
        match self.try_wait_for_update(TIMEOUT).await? {
            Some(items) => Ok(items),
            None => Ok(Vec::new()), // No update received, return empty
        }
    }

    /// Refresh and wait for the update (or handle timeout gracefully)
    pub async fn refresh_and_wait(&mut self) -> Result<Vec<T>, MailContextError> {
        self.refresh()?;

        // Try to get an update, but if none comes within reasonable time, return empty
        match self.try_wait_for_update(TIMEOUT).await? {
            Some(items) => Ok(items),
            None => Ok(Vec::new()), // No update received, return empty
        }
    }

    /// Get the current collected items
    pub fn items(&self) -> &[T] {
        &self.collected_items
    }

    /// Assert that the current items match the expected items
    pub fn assert_items(&self, expected: &[T]) {
        assert_eq!(self.collected_items, expected);
    }

    /// Handle a scroller update and update the collected items accordingly
    fn handle_scroller_update(
        &mut self,
        update: ScrollerUpdate<T>,
    ) -> Result<Option<Vec<T>>, MailContextError> {
        match update {
            ScrollerUpdate::None(_) => Ok(None),
            ScrollerUpdate::Append { src: _, items } => {
                self.collected_items.extend(items.clone());
                Ok(Some(items))
            }
            ScrollerUpdate::ReplaceFrom { src: _, idx, items } => {
                self.collected_items.splice(idx.., items.clone());
                Ok(Some(items))
            }
            ScrollerUpdate::ReplaceBefore { src: _, idx, items } => {
                self.collected_items.splice(..idx, items.clone());
                Ok(Some(items))
            }
            ScrollerUpdate::Error { src: _, error } => Err(error),
        }
    }
}

/// Convenience functions for creating TestScrollers for specific types
impl TestScroller<crate::datatypes::ContextualConversation> {
    /// Create a TestScroller for conversations
    pub async fn conversations(
        user_ctx: &std::sync::Arc<MailUserContext>,
        local_label_id: proton_core_common::datatypes::LocalLabelId,
        unread: crate::datatypes::ReadFilter,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let (scroller, handle) =
            MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
                .await?;
        Self::new(scroller, handle).await
    }
}

impl TestScroller<crate::models::Message> {
    /// Create a TestScroller for messages
    pub async fn messages(
        user_ctx: &std::sync::Arc<MailUserContext>,
        local_label_id: proton_core_common::datatypes::LocalLabelId,
        unread: crate::datatypes::ReadFilter,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let (scroller, handle) =
            MailScroller::messages(user_ctx.as_weak(), local_label_id, unread, page_size).await?;
        Self::new(scroller, handle).await
    }

    /// Create a TestScroller for search
    pub async fn search(
        user_ctx: &std::sync::Arc<MailUserContext>,
        search_options: crate::datatypes::SearchOptions,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let (scroller, handle) =
            MailScroller::search(user_ctx.as_weak(), search_options, page_size).await?;
        Self::new(scroller, handle).await
    }
}
