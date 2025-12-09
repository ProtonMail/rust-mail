//! Event data types for the Proton Mail common library.
//!
//! This module contains various data types used by the Proton Mail common
//! library in association to events. Notably, they are NOT used by the models
//! in the [`models`](crate::models) module, i.e. they do not represent child
//! data structures for the models' fields, and are entirely separate from the
//! persistent data structures.
//!
//! The data types used by events therefore do NOT need to be convertible to and
//! from database-compatible format using [`ToSql`](stash::exports::ToSql) and
//! [`FromSql`](stash::exports::FromSql). If anything in this module implements
//! those traits, it is a sign that a mistake has been made. Additionally, they
//! do not generally need to be serializable or deserializable, as they are not
//! used for network communication or any other interchange purpose as a general
//! requirement, and so implementation of [`Serialize`](serde::Serialize) and
//! [`Deserialize`](serde::Deserialize) is not necessary and may also be a sign
//! of a mistake.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need. Such conversions are usually very simple and indeed in many
//! cases can be done without altering any data in memory.
//!
//! This separation does cause some duplication, but the overlap is not total.
//! The various implementations for the types differ in each place; any logic
//! for the application is in the application types and not the API types; and
//! the distinction allows customisation of how the application deals with its
//! related data. Additionally, it promotes wider usability, as each application
//! that depends upon the API types can interpret and managed them in its own
//! way.
//!

use crate::datatypes::{ConversationLabelsCount, MessageLabelsCount};
use crate::models::{Conversation, IncomingDefaultEvent, MailSettings};
use proton_core_api::services::proton::EventId;
use proton_core_common::datatypes::Refresh;
use proton_core_common::event_loop::events::{Action, LabelEvent};
use proton_core_common::utils::MapVec as _;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    ConversationEvent as ApiConversationEvent, MailEvent as ApiMailEvent,
    MailEventV5 as ApiCombinedMailEvent, MessageEvent as ApiMessageEvent, MessageMetadata,
};

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationEvent {
    /// TODO: Document this field.
    pub remote_id: ConversationId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub conversation: Option<Conversation>,
}

impl From<ApiConversationEvent> for ConversationEvent {
    fn from(value: ApiConversationEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            conversation: value.conversation.map(Conversation::from),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MailEvent {
    /// TODO: Document this field.
    pub event_id: EventId,

    /// TODO: Document this field.
    pub conversation_counts: Option<Vec<ConversationLabelsCount>>,

    /// TODO: Document this field.
    pub conversations: Option<Vec<ConversationEvent>>,

    /// TODO: Document this field.
    pub has_more: bool,

    pub incoming_defaults: Option<Vec<IncomingDefaultEvent>>,

    /// TODO: Document this field.
    pub labels: Option<Vec<LabelEvent>>,

    /// TODO: Document this field.
    pub mail_settings: Option<MailSettings>,

    /// TODO: Document this field.
    pub message_counts: Option<Vec<MessageLabelsCount>>,

    /// TODO: Document this field.
    pub messages: Option<Vec<MessageEvent>>,

    /// Indicates whether we should refresh our data.
    pub refresh: Refresh,
}

impl From<ApiMailEvent> for MailEvent {
    fn from(value: ApiMailEvent) -> Self {
        Self {
            event_id: value.event_id,
            conversation_counts: value.conversation_counts.map(|conversation_counts| {
                conversation_counts
                    .into_iter()
                    .map(ConversationLabelsCount::from)
                    .collect()
            }),
            conversations: value.conversations.map(|conversations| {
                conversations
                    .into_iter()
                    .map(ConversationEvent::from)
                    .collect()
            }),
            labels: value.labels.map(|labels| labels.map_vec()),
            mail_settings: value.mail_settings.map(MailSettings::from),
            message_counts: value.message_counts.map(|message_counts| {
                message_counts
                    .into_iter()
                    .map(MessageLabelsCount::from)
                    .collect()
            }),
            messages: value.messages.map(|messages| messages.map_vec()),
            refresh: value.refresh.into(),
            has_more: value.has_more,
            incoming_defaults: value
                .incoming_defaults
                .map(|i| i.into_iter().map(Into::into).collect()),
        }
    }
}
impl From<ApiCombinedMailEvent> for MailEvent {
    fn from(value: ApiCombinedMailEvent) -> Self {
        Self {
            event_id: value.core.event_id,
            conversation_counts: value.conversation_counts.map(|conversation_counts| {
                conversation_counts
                    .into_iter()
                    .map(ConversationLabelsCount::from)
                    .collect()
            }),
            conversations: value.conversations.map(|conversations| {
                conversations
                    .into_iter()
                    .map(ConversationEvent::from)
                    .collect()
            }),
            labels: value.labels.map(|labels| labels.map_vec()),
            mail_settings: value.mail_settings.map(MailSettings::from),
            message_counts: value.message_counts.map(|message_counts| {
                message_counts
                    .into_iter()
                    .map(MessageLabelsCount::from)
                    .collect()
            }),
            messages: value.messages.map(|messages| messages.map_vec()),
            refresh: value.core.refresh.into(),
            has_more: value.core.has_more,
            incoming_defaults: value
                .incoming_defaults
                .map(|i| i.into_iter().map(Into::into).collect()),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageEvent {
    /// TODO: Document this field.
    pub remote_id: MessageId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub message: Option<MessageMetadata>,
}

impl From<ApiMessageEvent> for MessageEvent {
    fn from(value: ApiMessageEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            message: value.message,
        }
    }
}
