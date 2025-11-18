use std::{collections::HashMap, fmt::Display, sync::Arc, time::Duration};

use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, ModelExtension, ModelIdExtension},
};
use stash::{
    orm::Model,
    params,
    stash::{Bond, StashError, Tether},
};

use crate::{
    actions::ConversationOrMessage,
    conv_id, conversation,
    datatypes::{ContextualConversation, IncludeSwitch, ReadFilter, SearchOptions},
    label, lbl_id,
    mail_scroller::MailScrollerItem,
    message,
    models::{Conversation, ConversationCounters, Message, MessageCounters},
    msg_id,
    traits::ScrollerEq,
};

use super::utils::{create_address, test_address};

use crate::mail_scroller::ScrollerListUpdate;
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
        Message::apply_label_async(label_id, vec![local_message_id], bond)
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

    let conv = conv.clone();
    let labels = tether
        .sync_query(move |conn| conv.load_labels(conn))
        .await
        .unwrap();

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
        Conversation::apply_label_async(label_id, vec![local_conversation_id], bond)
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
pub struct TestScroller<T>
where
    T: MailScrollerItem,
{
    scroller: MailScroller<T>,
    handle: MailScrollerHandle<T>,
    collected_items: Vec<T>,
    updates: Vec<ScrollerUpdate<T>>,
}

impl<T> TestScroller<T>
where
    T: MailScrollerItem,
{
    pub async fn new(
        scroller: MailScroller<T>,
        handle: MailScrollerHandle<T>,
    ) -> Result<Self, MailContextError> {
        let mut test_scroller = Self::new_instant(scroller, handle);

        // Wait for any initial updates that might be available immediately
        // This handles cases where cached data is loaded during scroller initialization
        test_scroller
            .try_wait_for_update(Duration::from_secs(1))
            .await?;

        Ok(test_scroller)
    }

    pub fn new_instant(scroller: MailScroller<T>, handle: MailScrollerHandle<T>) -> Self {
        Self {
            scroller,
            handle,
            collected_items: Vec::new(),
            updates: Vec::new(),
        }
    }

    pub fn fetch_more(&self) -> Result<(), MailContextError> {
        self.scroller.fetch_more(None)
    }

    pub fn fetch_new(&self) -> Result<(), MailContextError> {
        self.scroller.fetch_new()
    }

    pub fn refresh(&self) -> Result<(), MailContextError> {
        self.scroller.refresh()
    }

    pub fn change_filter(&self, filter: ReadFilter) -> Result<(), MailContextError> {
        self.scroller.change_filter(filter)
    }

    pub fn change_label(&self, label: LocalLabelId) -> Result<(), MailContextError> {
        self.scroller.change_label(label)
    }

    pub fn change_include(&self, include: IncludeSwitch) -> Result<(), MailContextError> {
        self.scroller.change_include(include)
    }

    pub fn change_keywords(&self, keywords: SearchOptions) -> Result<(), MailContextError> {
        self.scroller.change_keywords(keywords)
    }

    pub fn force_refresh(&self) -> Result<(), MailContextError> {
        self.scroller.force_refresh()
    }

    pub async fn has_more(&self) -> Result<bool, MailContextError> {
        self.scroller.has_more().await
    }

    pub async fn total(&self) -> Result<u64, MailContextError> {
        self.scroller.total().await
    }

    pub async fn seen(&self) -> Result<u64, MailContextError> {
        self.scroller.seen().await
    }

    pub async fn wait_for_update(&mut self) -> Result<Option<Vec<T>>, MailContextError> {
        let update = loop {
            let update = self.handle.updates.recv_async().await.map_err(|_| {
                MailContextError::Other(anyhow::anyhow!("Failed to receive scroller update"))
            })?;
            if !update.is_status_update() {
                break update;
            }
        };

        self.handle_scroller_update(update)
    }

    pub async fn try_wait_for_update(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Vec<T>>, MailContextError> {
        match tokio::time::timeout(timeout, self.wait_for_update()).await {
            Ok(update) => update,
            _ => Ok(None),
        }
    }

    pub async fn fetch_more_and_wait(&mut self) -> Result<Vec<T>, MailContextError> {
        self.fetch_more()?;

        match self.try_wait_for_update(TIMEOUT).await? {
            Some(items) => Ok(items),
            None => Ok(Vec::new()),
        }
    }

    pub async fn fetch_new_and_wait(&mut self) -> Result<Vec<T>, MailContextError> {
        self.fetch_new()?;

        match self.try_wait_for_update(TIMEOUT).await? {
            Some(items) => Ok(items),
            None => Ok(Vec::new()),
        }
    }

    pub async fn refresh_and_wait(&mut self) -> Result<Vec<T>, MailContextError> {
        self.refresh()?;

        match self.try_wait_for_update(TIMEOUT).await? {
            Some(items) => Ok(items),
            None => Ok(Vec::new()),
        }
    }

    pub async fn match_next_update(&mut self, expected: TestUpdate) {
        let _ = self.try_wait_for_update(TIMEOUT).await;
        let actual = self.updates.last().unwrap();
        assert!(
            Self::assert_single_update(actual, &expected),
            "Expected update: {expected:?}, actual: {actual:?}"
        );
    }

    pub fn items(&self) -> &[T] {
        &self.collected_items
    }

    pub fn assert_items(&self, expected: &[T]) {
        assert!(self.collected_items.as_slice().scroller_eq(expected));
    }

    pub fn assert_updates(&self, expected: &[TestUpdate]) {
        assert_eq!(
            self.updates.len(),
            expected.len(),
            "Expected {} updates, got {}. Updates received so far: {:#?}",
            expected.len(),
            self.updates.len(),
            self.updates
        );
        for (actual, expected) in self.updates.iter().zip(expected.iter()) {
            assert!(
                Self::assert_single_update(actual, expected),
                "Expected update: {expected:?}, got {actual:?}.\nAll updates received so far: {:?}",
                self.updates
            );
        }
    }

    fn assert_single_update(actual: &ScrollerUpdate<T>, expected: &TestUpdate) -> bool {
        match (actual, expected) {
            (ScrollerUpdate::List(ScrollerListUpdate::None(_)), TestUpdate::None) => true,
            (
                ScrollerUpdate::List(ScrollerListUpdate::Append {
                    items: actual_items,
                    ..
                }),
                TestUpdate::Append {
                    items: expected_items,
                },
            ) => actual_items.len() == *expected_items,
            (
                ScrollerUpdate::List(ScrollerListUpdate::ReplaceFrom {
                    idx: actual_idx,
                    items: actual_items,
                    ..
                }),
                TestUpdate::ReplaceFrom {
                    idx: expected_idx,
                    items: expected_items,
                },
            ) => actual_idx == expected_idx && actual_items.len() == *expected_items,
            (
                ScrollerUpdate::List(ScrollerListUpdate::ReplaceBefore {
                    idx: actual_idx,
                    items: actual_items,
                    ..
                }),
                TestUpdate::ReplaceBefore {
                    idx: expected_idx,
                    items: expected_items,
                },
            ) => actual_idx == expected_idx && actual_items.len() == *expected_items,
            (
                ScrollerUpdate::List(ScrollerListUpdate::ReplaceRange {
                    from: actual_from,
                    to: actual_to,
                    items: actual_items,
                    ..
                }),
                TestUpdate::ReplaceRange {
                    from: expected_from,
                    to: expected_to,
                    items: expected_items,
                },
            ) => {
                actual_from == expected_from
                    && actual_to == expected_to
                    && actual_items.len() == *expected_items
            }
            (ScrollerUpdate::Error { src: _, error }, TestUpdate::Error(expected_error)) => {
                tracing::error!(
                    "Comparing error,\nactual: {},\nexpected: {}",
                    error.to_string(),
                    expected_error
                );
                &error.to_string() == expected_error
            }
            _ => false,
        }
    }

    fn handle_scroller_update(
        &mut self,
        update: ScrollerUpdate<T>,
    ) -> Result<Option<Vec<T>>, MailContextError> {
        tracing::info!("Scroller update: {update:?}");
        if !update.is_error() && !update.is_status_update() {
            self.updates.push(update.clone());
        }

        match update {
            ScrollerUpdate::List(update) => match update {
                ScrollerListUpdate::None(_) => Ok(None),
                ScrollerListUpdate::Append { src: _, items } => {
                    self.collected_items.extend(items.clone());
                    Ok(Some(items))
                }
                ScrollerListUpdate::ReplaceFrom { src: _, idx, items } => {
                    self.collected_items.splice(idx.., items.clone());
                    Ok(Some(items))
                }
                ScrollerListUpdate::ReplaceBefore { src: _, idx, items } => {
                    self.collected_items.splice(..idx, items.clone());
                    Ok(Some(items))
                }
                ScrollerListUpdate::ReplaceRange {
                    src: _,
                    from,
                    to,
                    items,
                } => {
                    self.collected_items.splice(from..to, items.clone());
                    Ok(Some(items))
                }
            },
            ScrollerUpdate::Error { src, error } => {
                let err_str = error.to_string();
                self.updates.push(ScrollerUpdate::Error { src, error });
                Err(MailContextError::Other(anyhow::anyhow!("Error: {err_str}")))
            }
            ScrollerUpdate::Status(_) => Ok(None),
        }
    }

    pub async fn supports_include_filter(&self) -> bool {
        self.scroller.supports_include_filter().await.unwrap()
    }
}

impl TestScroller<ContextualConversation> {
    pub async fn conversations(
        user_ctx: &Arc<MailUserContext>,
        local_label_id: LocalLabelId,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let (scroller, handle) =
            MailScroller::conversations(user_ctx.as_weak(), local_label_id, page_size).await?;

        Self::new(scroller, handle).await
    }

    pub async fn conversations_instant(
        user_ctx: &Arc<MailUserContext>,
        local_label_id: LocalLabelId,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let (scroller, handle) =
            MailScroller::conversations(user_ctx.as_weak(), local_label_id, page_size).await?;

        Ok(Self::new_instant(scroller, handle))
    }
}

impl TestScroller<Message> {
    pub async fn messages(
        user_ctx: &Arc<MailUserContext>,
        local_label_id: LocalLabelId,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let (scroller, handle) =
            MailScroller::messages(user_ctx.as_weak(), local_label_id, page_size).await?;

        Self::new(scroller, handle).await
    }

    pub async fn search(
        user_ctx: &Arc<MailUserContext>,
        options: SearchOptions,
        page_size: usize,
    ) -> Result<Self, MailContextError> {
        let (scroller, handle) =
            MailScroller::search(user_ctx.as_weak(), options, page_size).await?;

        Self::new(scroller, handle).await
    }
}

#[derive(Debug)]
pub enum TestUpdate {
    None,
    Append {
        items: usize,
    },
    ReplaceFrom {
        idx: usize,
        items: usize,
    },
    ReplaceBefore {
        idx: usize,
        items: usize,
    },
    ReplaceRange {
        from: usize,
        to: usize,
        items: usize,
    },
    Error(String),
}
