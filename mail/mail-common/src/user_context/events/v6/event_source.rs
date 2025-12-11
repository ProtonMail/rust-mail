use crate::MailContextError;
use crate::datatypes::dependencies::MessageOrConversationDependencyFetcher;
use crate::models::{Conversation, MailSettings};
use futures::StreamExt;
use futures::stream::FuturesOrdered;
use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{Action, LabelId, ProtonCore};
use proton_core_api::session::Session;
use proton_core_common::event_loop::v6::CoreEventSourceV6;
use proton_core_common::models::Label;
use proton_event_loop::v6::{EventSource, EventSourceDependencyList};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::{
    ConversationCount, GetConversationsOptions, MailEventV6, MessageCount, MessageMetadata,
};
use proton_mail_api::services::proton::requests::GetMessagesOptions;
use stash::stash::Tether;
use std::collections::{HashMap, HashSet};

pub struct MailEventSourceV6;

impl EventSource for MailEventSourceV6 {
    type Event = MailEventV6;
    type Cache = MailEventCache;

    fn name() -> &'static str {
        "mail-v6"
    }

    fn dependencies() -> EventSourceDependencyList {
        EventSourceDependencyList::default().with::<CoreEventSourceV6>()
    }
}

#[derive(Default)]
pub struct MailEventCache {
    conversations: HashMap<ConversationId, Conversation>,
    messages: HashMap<MessageId, MessageMetadata>,
    labels: HashMap<LabelId, Label>,
    settings: Option<MailSettings>,
    message_counters: HashMap<LabelId, MessageCount>,
    conversation_counters: HashMap<LabelId, ConversationCount>,
}

impl MailEventCache {
    #[tracing::instrument(skip_all)]
    pub async fn fetch_event_data(
        &mut self,
        session: &Session,
        event: &MailEventV6,
    ) -> Result<(), ApiServiceError> {
        if let Some(ref events) = event.labels {
            let ids = events
                .iter()
                .filter(|e| e.action != Action::Delete)
                .map(|e| e.id.clone());
            tracing::info!("Fetching labels");
            self.fetch_labels(session, ids).await?;
        }

        if let Some(ref events) = event.conversations {
            let ids = events
                .iter()
                .filter(|e| e.action != Action::Delete)
                .map(|e| e.id.clone());
            tracing::info!("Fetching Conversations");
            self.fetch_conversations(session, ids).await?;
        }

        if let Some(ref events) = event.messages {
            let ids = events
                .iter()
                .filter(|e| e.action != Action::Delete)
                .map(|e| e.id.clone());
            tracing::info!("Fetching Messages");
            self.fetch_messages(session, ids).await?;
        }

        if event.mail_settings.as_ref().is_some_and(|e| !e.is_empty()) {
            tracing::info!("Fetching Mail Settings");
            self.fetch_mail_settings(session).await?;
        }

        // calculate label counters to fetch
        let label_counter_ids = self
            .messages
            .values()
            .flat_map(|m| &m.label_ids)
            .cloned()
            .chain(
                self.conversations
                    .values()
                    .flat_map(|l| &l.labels)
                    .map(|l| l.remote_label_id.clone().expect("Is always set")),
            )
            .collect::<HashSet<LabelId>>();

        if !label_counter_ids.is_empty() {
            tracing::info!("Fetching message counters");
            self.fetch_message_counters(session, label_counter_ids.iter().cloned())
                .await?;
            tracing::info!("Fetching conversation counters");
            self.fetch_conversation_counters(session, label_counter_ids)
                .await?;
        }

        Ok(())
    }
    pub async fn fetch_labels(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = LabelId>,
    ) -> Result<(), ApiServiceError> {
        const MAX_LABELS_PER_REQUEST: usize = 50;
        let mut tasks = id
            .into_iter()
            .filter(|id| !self.labels.contains_key(id))
            .chunks(MAX_LABELS_PER_REQUEST)
            .into_iter()
            .map(|ids| session.get_labels_by_ids(ids.collect()))
            .collect::<FuturesOrdered<_>>();
        while let Some(task) = tasks.next().await {
            let response =
                task.inspect_err(|e| tracing::error!("Failed to fetch mail labels: {e}"))?;
            for label in response.labels {
                self.labels.insert(label.id.clone(), label.into());
            }
        }
        Ok(())
    }

    pub fn get_label_mut(&mut self, label_id: &LabelId) -> Option<&mut Label> {
        self.labels.get_mut(label_id)
    }

    pub async fn fetch_conversations(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = ConversationId>,
    ) -> Result<(), ApiServiceError> {
        const MAX_CONV_PER_REQUEST: usize = 50;
        let mut tasks = id
            .into_iter()
            .filter(|id| !self.conversations.contains_key(id))
            .chunks(MAX_CONV_PER_REQUEST)
            .into_iter()
            .map(|ids| {
                session.get_conversations(GetConversationsOptions {
                    address_id: None,
                    attachments: None,
                    auto_wildcard: None,
                    begin: None,
                    begin_id: None,
                    desc: None,
                    end: None,
                    end_id: None,
                    anchor: None,
                    anchor_id: None,
                    external_id: None,
                    from: None,
                    ids: Some(ids.collect()),
                    keyword: None,
                    label_id: None,
                    limit: None,
                    page: 0,
                    page_size: MAX_CONV_PER_REQUEST as u64,
                    recipients: None,
                    sort: None,
                    subject: None,
                    unread: None,
                })
            })
            .collect::<FuturesOrdered<_>>();
        while let Some(task) = tasks.next().await {
            let response =
                task.inspect_err(|e| tracing::error!("Failed to fetch conversations: {e}"))?;
            for conversation in response.conversations {
                tracing::debug!("Fetched {:?}", conversation.id);
                self.conversations
                    .insert(conversation.id.clone(), conversation.into());
            }
        }
        Ok(())
    }

    pub fn get_conversation_mut(&mut self, id: &ConversationId) -> Option<&mut Conversation> {
        self.conversations.get_mut(id)
    }

    pub async fn fetch_messages(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = MessageId>,
    ) -> Result<(), ApiServiceError> {
        const MAX_MSG_PER_REQUEST: usize = 50;
        let mut tasks = id
            .into_iter()
            .filter(|id| !self.messages.contains_key(id))
            .chunks(MAX_MSG_PER_REQUEST)
            .into_iter()
            .map(|ids| {
                session.get_messages(GetMessagesOptions {
                    address_id: None,
                    attachments: None,
                    auto_wildcard: None,
                    bcc: None,
                    begin: None,
                    begin_id: None,
                    cc: None,
                    conversation_id: None,
                    desc: None,
                    end: None,
                    end_id: None,
                    anchor: None,
                    anchor_id: None,
                    external_id: None,
                    from: None,
                    ids: Some(ids.collect()),
                    keyword: None,
                    label_id: None,
                    limit: None,
                    page: 0,
                    page_size: MAX_MSG_PER_REQUEST as u64,
                    recipients: None,
                    sort: None,
                    subject: None,
                    to: None,
                    unread: None,
                })
            })
            .collect::<FuturesOrdered<_>>();
        while let Some(task) = tasks.next().await {
            let response =
                task.inspect_err(|e| tracing::error!("Failed to fetch messages: {e}"))?;
            for message in response.messages {
                tracing::debug!("Fetched {:?}", message.id);
                self.messages.insert(message.id.clone(), message);
            }
        }
        Ok(())
    }

    pub fn get_message(&mut self, id: &MessageId) -> Option<&MessageMetadata> {
        self.messages.get(id)
    }

    pub async fn fetch_mail_settings(&mut self, session: &Session) -> Result<(), ApiServiceError> {
        if self.settings.is_none() {
            self.settings = Some(session.get_mail_settings().await?.mail_settings.into());
        }
        Ok(())
    }

    pub fn get_settings_mut(&mut self) -> Option<&mut MailSettings> {
        self.settings.as_mut()
    }

    pub async fn fetch_message_counters(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = LabelId>,
    ) -> Result<(), ApiServiceError> {
        const MAX_ITEMS_PER_REQUEST: usize = 50;
        let mut tasks = id
            .into_iter()
            .filter(|id| self.message_counters.contains_key(id))
            .chunks(MAX_ITEMS_PER_REQUEST)
            .into_iter()
            .map(|ids| session.get_messages_count_for_labels(ids.collect()))
            .collect::<FuturesOrdered<_>>();
        while let Some(task) = tasks.next().await {
            let response =
                task.inspect_err(|e| tracing::error!("Failed to fetch message counters: {e}"))?;
            for count in response.counts {
                self.message_counters.insert(count.label_id.clone(), count);
            }
        }
        Ok(())
    }

    pub async fn fetch_conversation_counters(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = LabelId>,
    ) -> Result<(), ApiServiceError> {
        const MAX_ITEMS_PER_REQUEST: usize = 50;
        let mut tasks = id
            .into_iter()
            .filter(|id| self.conversation_counters.contains_key(id))
            .chunks(MAX_ITEMS_PER_REQUEST)
            .into_iter()
            .map(|ids| session.get_conversations_count_for_labels(ids.collect()))
            .collect::<FuturesOrdered<_>>();
        while let Some(task) = tasks.next().await {
            let response = task
                .inspect_err(|e| tracing::error!("Failed to fetch conversation counters: {e}"))?;
            for count in response.counts {
                self.conversation_counters
                    .insert(count.label_id.clone(), count);
            }
        }
        Ok(())
    }

    pub async fn calculate_missing_dependencies(
        &self,
        tether: &Tether,
    ) -> Result<MessageOrConversationDependencyFetcher, MailContextError> {
        let mut fetcher = MessageOrConversationDependencyFetcher::new();
        for conv in self.conversations.values() {
            fetcher.check_conversation(conv, tether).await?
        }

        for msg in self.messages.values() {
            fetcher.check_api_message_metadata(msg, tether).await?
        }

        Ok(fetcher)
    }

    pub fn get_message_counts(&self) -> impl Iterator<Item = &MessageCount> + use<'_> {
        self.message_counters.values()
    }

    pub fn get_conversation_counts(&self) -> impl Iterator<Item = &ConversationCount> + use<'_> {
        self.conversation_counters.values()
    }
}
