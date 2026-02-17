use crate::MailContextError;
use crate::datatypes::dependencies::DependencyFetcher;
use crate::models::{Conversation, MailSettings};
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
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
use tracing::error;

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
    settings: Option<Box<MailSettings>>,
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
        let mut tasks = FuturesUnordered::<FutureTask>::new();

        if let Some(ref events) = event.labels {
            let ids = events
                .iter()
                .filter(|e| e.action != Action::Delete)
                .map(|e| e.id.clone());
            tracing::info!("Fetching labels");
            self.fetch_labels(&mut tasks, session, ids);
        }

        if let Some(ref events) = event.conversations {
            let ids = events
                .iter()
                .filter(|e| e.action != Action::Delete)
                .map(|e| e.id.clone());
            tracing::info!("Fetching Conversations");
            self.fetch_conversations(&mut tasks, session, ids);
        }

        if let Some(ref events) = event.messages {
            let ids = events
                .iter()
                .filter(|e| e.action != Action::Delete)
                .map(|e| e.id.clone());
            tracing::info!("Fetching Messages");
            self.fetch_messages(&mut tasks, session, ids);
        }

        if event.mail_settings.as_ref().is_some_and(|e| !e.is_empty()) {
            tracing::info!("Fetching Mail Settings");
            self.fetch_mail_settings(&mut tasks, session);
        }

        let mut first_err = None;
        while let Some(result) = tasks.next().await {
            match result {
                Ok(data) => data.apply(self),
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }

        if let Some(first_err) = first_err {
            return Err(first_err);
        }

        // For new labels we need to fetch counters
        let mut label_counter_ids = event
            .labels
            .as_ref()
            .map(|l| {
                l.iter()
                    .filter_map(|event| {
                        (event.action == Action::Create).then_some(event.id.clone())
                    })
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();

        // calculate label counters to fetch
        label_counter_ids.extend(
            self.messages
                .values()
                .flat_map(|m| &m.label_ids)
                .cloned()
                .chain(
                    self.conversations
                        .values()
                        .flat_map(|l| &l.labels)
                        .map(|l| l.remote_label_id.clone().expect("Is always set")),
                ),
        );

        if !label_counter_ids.is_empty() {
            tracing::info!("Fetching counters");
            let mut tasks = FuturesUnordered::<FutureTask>::new();
            self.fetch_message_counters(&mut tasks, session, label_counter_ids.iter().cloned());
            self.fetch_conversation_counters(&mut tasks, session, label_counter_ids);
            while let Some(result) = tasks.next().await {
                result?.apply(self);
            }
        }

        Ok(())
    }
    fn fetch_labels(
        &mut self,
        tasks: &mut FuturesUnordered<FutureTask>,
        session: &Session,
        ids: impl IntoIterator<Item = LabelId>,
    ) {
        const MAX_LABELS_PER_REQUEST: usize = 50;
        tasks.extend(
            ids.into_iter()
                .filter(|id| !self.labels.contains_key(id))
                .chunks(MAX_LABELS_PER_REQUEST)
                .into_iter()
                .map(|ids| -> FutureTask {
                    let session = session.clone();
                    let ids = ids.collect();
                    Box::pin(async move {
                        session
                            .get_labels_by_ids(ids)
                            .await
                            .inspect_err(|e| error!("Failed to fetch labels: {e}"))
                            .map(|r| {
                                FetchData::Label(r.labels.into_iter().map(Into::into).collect())
                            })
                    })
                }),
        );
    }

    pub fn get_label_mut(&mut self, label_id: &LabelId) -> Option<&mut Label> {
        self.labels.get_mut(label_id)
    }

    pub fn get_label(&self, label_id: &LabelId) -> Option<&Label> {
        self.labels.get(label_id)
    }

    fn fetch_conversations(
        &mut self,
        tasks: &mut FuturesUnordered<FutureTask>,
        session: &Session,
        ids: impl IntoIterator<Item = ConversationId>,
    ) {
        const MAX_CONV_PER_REQUEST: usize = 50;
        tasks.extend(
            ids.into_iter()
                .filter(|id| !self.conversations.contains_key(id))
                .chunks(MAX_CONV_PER_REQUEST)
                .into_iter()
                .map(|ids| -> FutureTask {
                    let session = session.clone();
                    let ids = ids.collect();
                    Box::pin(async move {
                        session
                            .get_conversations(GetConversationsOptions {
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
                                ids: Some(ids),
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
                            .await
                            .inspect_err(|e| error!("Failed to get conversations: {e}"))
                            .map(|r| {
                                FetchData::Conversation(
                                    r.conversations.into_iter().map(Into::into).collect(),
                                )
                            })
                    })
                }),
        );
    }

    pub fn get_conversation_mut(&mut self, id: &ConversationId) -> Option<&mut Conversation> {
        self.conversations.get_mut(id)
    }

    fn fetch_messages(
        &self,
        tasks: &mut FuturesUnordered<FutureTask>,
        session: &Session,
        ids: impl IntoIterator<Item = MessageId>,
    ) {
        const MAX_MSG_PER_REQUEST: usize = 50;
        tasks.extend(
            ids.into_iter()
                .filter(|id| !self.messages.contains_key(id))
                .chunks(MAX_MSG_PER_REQUEST)
                .into_iter()
                .map(|ids| -> FutureTask {
                    let session = session.clone();
                    let ids = ids.collect();
                    Box::pin(async move {
                        session
                            .get_messages(GetMessagesOptions {
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
                                ids: Some(ids),
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
                            .await
                            .inspect_err(|e| error!("Failed to get messages: {e}"))
                            .map(|r| FetchData::Messages(r.messages))
                    })
                }),
        );
    }

    pub fn get_message(&mut self, id: &MessageId) -> Option<&MessageMetadata> {
        self.messages.get(id)
    }

    fn fetch_mail_settings(&self, tasks: &mut FuturesUnordered<FutureTask>, session: &Session) {
        if self.settings.is_none() {
            let session = session.clone();
            tasks.push(Box::pin(async move {
                session
                    .get_mail_settings()
                    .await
                    .inspect_err(|e| error!("Failed to fetch mail settings: {e}"))
                    .map(|s| FetchData::MailSettings(Box::new(s.mail_settings.into())))
            }));
        }
    }

    pub fn get_settings_mut(&mut self) -> Option<&mut MailSettings> {
        self.settings.as_mut().map(|v| v.as_mut())
    }

    fn fetch_message_counters(
        &mut self,
        tasks: &mut FuturesUnordered<FutureTask>,
        session: &Session,
        ids: impl IntoIterator<Item = LabelId>,
    ) {
        const MAX_ITEMS_PER_REQUEST: usize = 50;
        tasks.extend(
            ids.into_iter()
                .filter(|id| !self.message_counters.contains_key(id))
                .chunks(MAX_ITEMS_PER_REQUEST)
                .into_iter()
                .map(|ids| -> FutureTask {
                    let session = session.clone();
                    let ids = ids.collect::<Vec<_>>();
                    Box::pin(async move {
                        session
                            .get_messages_count_for_labels(ids)
                            .await
                            .inspect_err(|e| error!("Failed to fetch message counters: {e}"))
                            .map(|r| FetchData::MessagesCount(r.counts))
                    })
                }),
        );
    }

    fn fetch_conversation_counters(
        &mut self,
        tasks: &mut FuturesUnordered<FutureTask>,
        session: &Session,
        ids: impl IntoIterator<Item = LabelId>,
    ) {
        const MAX_ITEMS_PER_REQUEST: usize = 50;
        tasks.extend(
            ids.into_iter()
                .filter(|id| !self.conversation_counters.contains_key(id))
                .chunks(MAX_ITEMS_PER_REQUEST)
                .into_iter()
                .map(|ids| -> FutureTask {
                    let session = session.clone();
                    let ids = ids.collect();
                    Box::pin(async move {
                        session
                            .get_conversations_count_for_labels(ids)
                            .await
                            .inspect_err(|e| error!("Failed to fetch conversation counters: {e}"))
                            .map(|r| FetchData::ConversationCount(r.counts))
                    })
                }),
        );
    }

    pub async fn calculate_missing_dependencies(
        &self,
        tether: &Tether,
    ) -> Result<DependencyFetcher, MailContextError> {
        let mut fetcher = DependencyFetcher::new();
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

type FutureTask = BoxFuture<'static, Result<FetchData, ApiServiceError>>;

#[derive(Debug)]
enum FetchData {
    Label(Vec<Label>),
    Conversation(Vec<Conversation>),
    Messages(Vec<MessageMetadata>),
    MailSettings(Box<MailSettings>),
    ConversationCount(Vec<ConversationCount>),
    MessagesCount(Vec<MessageCount>),
}

impl FetchData {
    fn apply(self, cache: &mut MailEventCache) {
        match self {
            FetchData::Label(labels) => {
                for label in labels {
                    cache
                        .labels
                        .insert(label.remote_id.clone().expect("Should be set"), label);
                }
            }
            FetchData::Conversation(conversations) => {
                for conversation in conversations {
                    cache.conversations.insert(
                        conversation.remote_id.clone().expect("Should be set"),
                        conversation,
                    );
                }
            }
            FetchData::Messages(messages) => {
                for message in messages {
                    cache.messages.insert(message.id.clone(), message);
                }
            }
            FetchData::MailSettings(settings) => {
                cache.settings = Some(settings);
            }
            FetchData::ConversationCount(counters) => {
                for counter in counters {
                    cache
                        .conversation_counters
                        .insert(counter.label_id.clone(), counter);
                }
            }
            FetchData::MessagesCount(counters) => {
                for counter in counters {
                    cache
                        .message_counters
                        .insert(counter.label_id.clone(), counter);
                }
            }
        }
    }
}
